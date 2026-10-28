//! Architecture guards of `stokesforge-app` that read the code (#116, work
//! package #115).
//!
//! None of them names a file: all read the same recursive list of `src/`
//! (`support/sources.rs`), and look for a function **by its name**, wherever it
//! lives. Moving code from `app.rs` to `src/app/` takes no rule out of its
//! guard; an expected function that is no longer found makes the guard fail
//! instead of letting it pass silently.
//!
//! - `no_window_is_a_raw_egui_window`: every window goes through the common
//!   building block (`show_window_in_layer`, `modal_dialog_with`);
//! - `the_properties_window_is_built_like_the_others` (#13);
//! - `no_global_import_outside_tests`: explicit imports, no `use …::*` outside
//!   test code (decision 2 of #115), no exemption;
//! - `no_raw_separator_in_a_menu` (#155): a menu rule is placed only by the
//!   sections building block (`menu_sections.rs`), no exemption;
//! - `no_selectable_button_outside_a_menu` (#169): egui's selected button
//!   background is reserved for the exclusive choices of a menu, a combo box or
//!   the color picker; a list goes through `list_row`, a set of tabs through
//!   `tab_strip`;
//! - `no_hardcoded_color_outside_the_theme` (#169): an interface color comes
//!   from `theme.rs` (or the `Visuals` it sets), never from a
//!   `Color32::from_rgb` written in a surface;
//! - `no_numeric_field_outside_the_building_block` (#220): every typed number
//!   goes through `number_field::number_field` and the quantities table, never
//!   through an egui `DragValue` or `Slider`, no exemption;
//! - `no_separator_before_a_section_title` (#169): a section title
//!   (`theme::section_title`) separates its groups alone;
//! - `no_literal_add_space_outside_the_theme` (#227, S-L4): a gap is placed
//!   with the two constants of `theme.rs` (`SPACE_ITEM`, `SPACE_SECTION`),
//!   never with a number written in place, no exemption;
//! - `no_consume_key_outside_the_registry` (#231, S-K1): every consumed key is
//!   a constant of `app/shortcuts.rs`, never a `consume_key` nor a
//!   `KeyboardShortcut::new` written elsewhere, no exemption;
//! - `no_cursor_outside_the_cursors_module` (#231, S-X2): a
//!   `CursorIcon::…` is written only in `app/cursors.rs`, no exemption;
//! - `no_rfd_picker_outside_the_module` (#192): an `rfd::…` is written only
//!   in `file_picker.rs`, which lives in `stokesforge-app-state` (#263);
//! - `no_tool_picker_outside_the_tools_bar` (#299, D1, D5): a choice
//!   whose arguments name a tool is not in a window; exception
//!   through `// TOOL-IN-WINDOW(#299): <justification>` written just before;
//! - `no_gesture_outside_the_matrix` (#299, D4): every file that reads a
//!   mouse wheel or a double click is cited by `docs/gestures.md`;
//! - `no_free_size_without_justification` (#300, S-F11): an interactive window
//!   takes a size class; a free size (`InteractiveSize::Free`) requires
//!   `// FREE-SIZE(#300): <justification>` just before;
//! - `the_window_catalog_and_the_code_agree` (#300, S-F11): every
//!   `stokesforge_…` identifier passed to a `modal_*` function appears in
//!   `docs/windows.md`, and conversely; does not check the sizes.
//!
//! The old guard "the state modules do not name egui" (#255) is gone: since
//! #263 these modules live in `stokesforge-app-state`, which depends on no
//! interface crate, and the compiler refuses an `egui::` (as well as a
//! `crate::app` — #128 — in the state).
//!
//! The sources read are those of both crates: `stokesforge-app` (`app.rs`,
//! `app/menus.rs`) and `stokesforge-app-state` (prefix `app-state/`:
//! `app-state/state.rs`).
//!
//! Each guard has its **witness**: the forbidden pattern, placed in a file
//! other than `app.rs`, is indeed refused — without it, a guard that sees
//! nothing would pass for green. The witnesses of the source reader itself are
//! at the end of the file.

#[path = "support/sources.rs"]
mod sources;

use sources::{
    body, closing, functions, ident_at, is_ident, line_of, short_name, starts_with_at,
    unique_function, word_at, SourceFile,
};

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.to_string(),
        text: text.to_string(),
        is_test: false,
    }
}

// ---------------------------------------------------------------------------
// No raw `egui::Window`
// ---------------------------------------------------------------------------

/// The `Window::new(` constructions of the production code, `path:line`.
/// Every form is seen: `egui::Window::new(`, and `Window::new(` after a
/// `use egui::Window`.
fn raw_windows(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        for i in 0..source.mask.len() {
            if word_at(&source.mask, i, "Window")
                && starts_with_at(&source.mask, i + "Window".len(), "::new(")
            {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Project rule: **every** window of the application goes through the common
/// building block (`show_window_in_layer`, via `modal_dialog_with` for a
/// dialog), never through a raw `egui::Window`. The import wizard once departed
/// from this rule: this guard prevents it from happening again.
#[test]
fn no_window_is_a_raw_egui_window() {
    let sources = sources::sources();
    let offenders = raw_windows(&sources);
    assert!(
        offenders.is_empty(),
        "a window is built by a raw `egui::Window`: go through \
         `show_window_in_layer` / `modal_dialog_with` (see `stokesforge-feature`): {offenders:?}"
    );
    let import = unique_function(&sources, "import_window");
    assert!(
        import.body.contains("modal_wizard("),
        "the import wizard ({}:{}) is a modal dialog",
        import.path,
        import.line
    );
}

/// The witness: a raw window written outside `app.rs`, in each of its forms,
/// is refused; what builds none passes.
#[test]
fn the_windows_guard_sees_a_raw_window_outside_app_rs() {
    for code in [
        "fn f(ctx: &Context) { egui::Window::new(title).show(ctx, |_| {}); }",
        "use egui::Window;\nfn f(ctx: &Context) { Window::new(title).show(ctx, |_| {}); }",
        "fn f() { eframe::egui::containers::Window::new(t); }",
        // Production code placed after a test module is still read.
        "#[cfg(test)]\nmod tests {}\nfn f() { egui::Window::new(t); }",
    ] {
        let found = raw_windows(&[file("app/import.rs", code)]);
        assert_eq!(found.len(), 1, "not seen: {code}");
        assert!(found[0].starts_with("src/app/import.rs:"), "{found:?}");
    }
    for code in [
        "// egui::Window::new(t)\nfn f() {}",
        "fn f() { let _ = \"egui::Window::new(\"; }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g() { egui::Window::new(t); } }",
        "fn f(n: &Node) { if let ProjectTreeNode::Window { id, .. } = n {} }",
        "fn f() { MyWindow::new(t); }",
    ] {
        assert!(
            raw_windows(&[file("app/import.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
}

/// #13: the properties window does not write its own frame — it goes through
/// `modal_dialog`, which delegates to `modal_dialog_with`, which places the
/// window through the content windows building block (`show_window_in_layer`,
/// `window_header`). Each function is looked up by name, wherever it lives.
#[test]
fn the_properties_window_is_built_like_the_others() {
    let sources = sources::sources();
    let properties = unique_function(&sources, "plot_settings_window");
    assert!(
        !properties.body.contains("Window::new("),
        "no raw Window anymore"
    );
    assert!(properties.body.contains("modal_dialog("));
    let dialog = unique_function(&sources, "modal_dialog");
    assert!(dialog.body.contains("modal_dialog_with("));
    let building_block = unique_function(&sources, "modal_dialog_with");
    assert!(building_block.body.contains("modal_frame("));
    // The wizard and the read-only window (#228), the confirmations
    // and the serious failure (#229): the same body.
    for variante in [
        "modal_wizard",
        "modal_live_settings",
        "modal_read_only",
        "modal_read_only_with",
        "modal_confirm",
        "modal_save_changes",
        "modal_alert",
    ] {
        assert!(unique_function(&sources, variante)
            .body
            .contains("modal_frame("));
    }
    let body = unique_function(&sources, "modal_frame");
    assert!(body.body.contains("show_window_in_layer("));
    assert!(body.body.contains("window_header("));
}

/// The witness of the search by name: a function moved into
/// `src/app/…` is found; a test copy does not count; a function that
/// became a method is still found; an array signature (`[u8; 4]`) is not
/// taken for a declaration without a body.
#[test]
fn a_function_is_found_outside_app_rs_by_name() {
    let sources = [
        file("app.rs", "impl App { fn other(&self) { modal_dialog_with(); } }"),
        file(
            "app/dialog.rs",
            "pub(crate) fn modal_dialog(ctx: &Context) {\n    modal_dialog_with(ctx); // window_header(\n}\n\
             #[cfg(test)]\nmod tests { fn modal_dialog() {} }",
        ),
    ];
    let found = unique_function(&sources, "modal_dialog");
    assert_eq!(found.path, "app/dialog.rs");
    assert_eq!(found.line, 1);
    assert!(found.body.contains("modal_dialog_with("));
    assert!(
        !found.body.contains("window_header("),
        "a comment is not a call"
    );
    let method = [file(
        "app/windows.rs",
        "impl StokesForgeApp {\n    fn import_window(&mut self, keys: [u8; 4]) -> [u8; 2] {\n        modal_dialog_with();\n    }\n}",
    )];
    let found = unique_function(&method, "import_window");
    assert_eq!(found.qualified, "StokesForgeApp::import_window");
    assert!(found.body.contains("modal_dialog_with("));
}

/// A missing function makes the guard fail: it does not leave the rule
/// silently.
#[test]
#[should_panic(expected = "found 0 times")]
fn a_missing_function_fails_the_guard() {
    unique_function(&[file("app/grid.rs", "fn other() {}")], "import_window");
}

/// Two free functions of the same name (two modules): the guard does not choose.
#[test]
#[should_panic(expected = "found 2 times")]
fn a_duplicate_function_fails_the_guard() {
    unique_function(
        &[
            file("app/a.rs", "fn import_window() {}"),
            file("app/b.rs", "fn import_window() {}"),
        ],
        "import_window",
    );
}

// ---------------------------------------------------------------------------
// No global import outside tests
// ---------------------------------------------------------------------------

/// A global import: its file, its line, its import tree without whitespace
/// (`super::*`, `super::{a,b::*}`).
#[derive(Debug, PartialEq, Eq)]
struct GlobalImport {
    path: String,
    line: usize,
    tree: String,
}

/// The `use …*` of the production code (`pub use` included).
fn global_imports(sources: &[SourceFile]) -> Vec<GlobalImport> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        let mask = &source.mask;
        for i in 0..mask.len() {
            if !word_at(mask, i, "use") {
                continue;
            }
            let end = (i..mask.len())
                .find(|k| mask[*k] == ';')
                .unwrap_or(mask.len());
            let tree: String = mask[i + 3..end]
                .iter()
                .filter(|c| !c.is_whitespace())
                .collect();
            if tree.contains('*') {
                found.push(GlobalImport {
                    path: file.path.clone(),
                    line: line_of(&source.original, i),
                    tree,
                });
            }
        }
    }
    found
}

/// The deviations, one per global import; empty if everything is in order.
fn import_deviations(sources: &[SourceFile]) -> Vec<String> {
    global_imports(sources)
        .iter()
        .map(|t| {
            format!(
                "src/{}:{}: `use {};` — global import outside tests: name what \
                 you import (decision 2 of #115)",
                t.path, t.line, t.tree
            )
        })
        .collect()
}

/// Decision 2 of #115: real modules, with explicit imports. No
/// `use …::*` — `use super::*` at the top — outside `#[cfg(test)]` code, in
/// all of `src/` (not only `src/app/`): a `use super::*` in production
/// turns a module into a mere file split, which sees everything of its
/// parent without declaring anything. Absolute guard: the exemptions of the
/// `window_*.rs` files of #101 were lifted by #117.
#[test]
fn no_global_import_outside_tests() {
    let deviations = import_deviations(&sources::sources());
    assert!(
        deviations.is_empty(),
        "{} deviation(s):\n  {}",
        deviations.len(),
        deviations.join("\n  ")
    );
}

/// The witness: each form of global import, placed in a module of
/// `src/app/`, is refused.
#[test]
fn the_imports_guard_sees_every_form_of_global_import() {
    for (code, tree, line) in [
        ("use super::*;", "super::*", 1),
        ("pub(crate) use super::*;", "super::*", 1),
        ("use super::grid::*;", "super::grid::*", 1),
        ("use super::{Tab, grid::*};", "super::{Tab,grid::*}", 1),
        ("use crate::state::*;", "crate::state::*", 1),
        ("use egui::*;", "egui::*", 1),
        ("fn f() {\n    use LineStyle::*;\n}", "LineStyle::*", 2),
        // Code placed after a test module remains production code
        // (`errors.rs` alternates the two).
        (
            "mod a {\n    #[cfg(test)]\n    mod tests {\n        use super::*;\n    }\n}\nuse super::*;",
            "super::*",
            7,
        ),
    ] {
        let found = global_imports(&[file("app/import.rs", code)]);
        assert_eq!(found.len(), 1, "not seen: {code}");
        assert_eq!(found[0].path, "app/import.rs");
        assert_eq!(found[0].tree, tree);
        assert_eq!(found[0].line, line, "{code}");
    }
}

#[test]
fn the_imports_guard_lets_test_code_and_named_imports_pass() {
    for code in [
        "use super::{Tab, WindowKind};",
        "use std::ops::Mul;",
        "// use super::*;",
        "fn f() { let _ = \"use super::*;\"; }",
        "#[cfg(test)]\nmod tests {\n    use super::*;\n}",
        "#[cfg(test)]\nuse super::*;",
    ] {
        assert!(
            global_imports(&[file("app/import.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
    let mut test_files = file("app/import/tests.rs", "use super::*;");
    test_files.is_test = true;
    assert!(global_imports(&[test_files]).is_empty());
}

/// The report names each import, one deviation per import — two in the same
/// file make two deviations.
#[test]
fn the_imports_guard_reports_every_import() {
    let sources = [
        file(
            "app/window_chrome.rs",
            "use super::*;\nfn f() {}\nuse super::*;",
        ),
        file("app/grid.rs", "use crate::state::{Action, *};"),
    ];
    let deviations = import_deviations(&sources);
    assert_eq!(deviations.len(), 3, "{deviations:?}");
    assert!(
        deviations[0].starts_with("src/app/window_chrome.rs:1"),
        "{deviations:?}"
    );
    assert!(
        deviations[1].starts_with("src/app/window_chrome.rs:3"),
        "{deviations:?}"
    );
    assert!(
        deviations[2].starts_with("src/app/grid.rs:1")
            && deviations[2].contains("crate::state::{Action,*}"),
        "{deviations:?}"
    );
}

// ---------------------------------------------------------------------------
// The source reader
// ---------------------------------------------------------------------------

fn write_file(root: &std::path::Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("directory")).expect("directory");
    std::fs::write(path, text).expect("write");
}

/// Reading descends into subdirectories, reads only the `.rs` files, and returns
/// sorted relative paths.
#[test]
fn reading_descends_into_subdirectories() {
    let dir = tempfile::tempdir().expect("temporary directory");
    write_file(dir.path(), "lib.rs", "mod app;");
    write_file(dir.path(), "app.rs", "mod grid;");
    write_file(dir.path(), "app/grid.rs", "");
    write_file(dir.path(), "app/grid/cell.rs", "");
    write_file(dir.path(), "app/notes.md", "");
    let paths: Vec<String> = sources::read_sources(dir.path())
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(
        paths,
        ["app.rs", "app/grid.rs", "app/grid/cell.rs", "lib.rs"]
    );
}

/// On the real repository: the subdirectories of `src/` are read (`i18n/pseudo.rs`
/// escaped `no_hardcoded_label` before #116), those of
/// `stokesforge-app-state/src/` too (prefix `app-state/`, #263), and no
/// production file is taken for test code.
#[test]
fn real_reading_sees_the_src_subdirectories() {
    let sources = sources::sources();
    for expected in [
        "app.rs",
        "lib.rs",
        "app/window_chrome.rs",
        // The state, in its neighbor crate (#263): prefix `app-state/`.
        "app-state/state.rs",
        "app-state/i18n/pseudo.rs",
    ] {
        let file = sources
            .iter()
            .find(|f| f.path == expected)
            .unwrap_or_else(|| panic!("{expected} not read"));
        assert!(!file.is_test, "{expected} taken for test code");
    }
}

/// A test module written in its own file (`#[cfg(test)] mod x;`,
/// with or without `#[path]`, attribute before or after) makes that whole file
/// test code, like the modules it declares in turn; the others stay
/// production.
#[test]
fn a_test_module_in_its_own_file_is_test_code() {
    let dir = tempfile::tempdir().expect("temporary directory");
    write_file(dir.path(), "lib.rs", "mod app;\n#[cfg(test)]\nmod tests;\n");
    write_file(
        dir.path(),
        "app.rs",
        "#[path = \"app_tests.rs\"]\n#[cfg(test)]\nmod t;\n#[cfg(test)] pub(crate) mod help;\nmod grid;\n",
    );
    write_file(dir.path(), "app/grid.rs", "use super::*;");
    write_file(dir.path(), "app/help.rs", "use super::*;");
    write_file(dir.path(), "app_tests.rs", "use super::*;");
    write_file(dir.path(), "tests.rs", "mod tools;\n");
    write_file(dir.path(), "tests/tools.rs", "use super::*;");
    let sources = sources::read_sources(dir.path());
    let test_files: Vec<&str> = sources
        .iter()
        .filter(|f| f.is_test)
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(
        test_files,
        ["app/help.rs", "app_tests.rs", "tests.rs", "tests/tools.rs"]
    );
    // And the imports guard sees only the production module.
    let found = global_imports(&sources);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, "app/grid.rs");
}

/// The production view blanks **every** `#[cfg(test)]` item — module,
/// function, `use` — and keeps the code that follows them as well as the line
/// numbers.
#[test]
fn production_blanks_every_test_item_and_keeps_the_rest() {
    let text = "fn avant() {}\n\
                 #[cfg(test)]\n\
                 mod tests {\n    fn TEST_ONE() {}\n}\n\
                 fn milieu() {}\n\
                 #[cfg(test)]\n#[test]\nfn TEST_TWO() { let _ = [0u8; 2]; }\n\
                 #[cfg(test)] use TEST_THREE;\n\
                 fn apres() {}\n";
    let production = file("app/a.rs", text).production();
    let code = production.code_as_text();
    for needle in ["fn avant", "fn milieu", "fn apres"] {
        assert!(code.contains(needle), "{needle} lost:\n{code}");
    }
    assert!(!code.contains("TEST_"), "test code remains:\n{code}");
    assert_eq!(code.lines().count(), text.lines().count());
    let line_after = code.lines().position(|l| l.contains("fn apres"));
    assert_eq!(
        line_after,
        text.lines().position(|l| l.contains("fn apres"))
    );
}

/// A `#[cfg(test)]` on a field, a `match` arm or an argument blanks only that
/// element: the production code that follows it is still read.
#[test]
fn a_cfg_test_outside_an_item_does_not_swallow_the_rest() {
    for text in [
        "pub struct S {\n    a: u8,\n    #[cfg(test)]\n    pub TEST_FIELD: u8,\n}\nfn PRODUCTION() {}\n",
        "pub struct S {\n    #[cfg(test)]\n    TEST_LAST: HashMap<u8, u8>\n}\nfn PRODUCTION() {}\n",
        "fn f(x: u8) -> u8 {\n    match x {\n        #[cfg(test)]\n        0 => TEST_ARM,\n        _ => { PRODUCTION }\n    }\n}\n",
        "fn f() -> S {\n    S {\n        #[cfg(test)]\n        hook: TEST_INIT,\n        a: PRODUCTION,\n    }\n}\n",
        "fn f() {\n    appel(\n        #[cfg(test)]\n        TEST_ARG,\n    );\n    PRODUCTION();\n}\n",
    ] {
        let code = file("app/a.rs", text).production().code_as_text();
        assert!(code.contains("PRODUCTION"), "production swallowed:\n{code}");
        assert!(!code.contains("TEST_"), "test code remains:\n{code}");
    }
}

/// A `#![cfg(test)]` at the top of a file makes it entirely test code, and the
/// modules it declares with it; a `#[path]` that leaves `src/` designates no
/// source read.
#[test]
fn an_inner_attribute_and_a_path_outside_src_are_understood() {
    let dir = tempfile::tempdir().expect("temporary directory");
    write_file(
        dir.path(),
        "lib.rs",
        "mod tools;\nmod trials;\n#[cfg(test)]\n#[path = \"../tools.rs\"]\nmod h;\n",
    );
    write_file(dir.path(), "tools.rs", "use super::*;");
    write_file(
        dir.path(),
        "trials.rs",
        "//! Trials.\n#![allow(dead_code)]\n#![cfg(test)]\nuse super::*;\nmod help;\n",
    );
    write_file(dir.path(), "trials/help.rs", "use super::*;");
    let sources = sources::read_sources(dir.path());
    let test_files: Vec<&str> = sources
        .iter()
        .filter(|f| f.is_test)
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(test_files, ["trials.rs", "trials/help.rs"]);
    let found = global_imports(&sources);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, "tools.rs");
}

// ---------------------------------------------------------------------------
// No raw separator in a menu (#155)
// ---------------------------------------------------------------------------

/// The code ranges that paint a menu: the body of every function whose name
/// contains "menu", and the arguments of `context_menu(`, `menu_button(` and
/// `SubMenuButton…::ui(`.
fn menu_zones(mask: &[char]) -> Vec<(usize, usize)> {
    let mut zones = Vec::new();
    for (position, name) in functions(mask) {
        if short_name(&name).contains("menu") {
            zones.extend(body(mask, position));
        }
    }
    for pattern in ["context_menu(", "menu_button(", "SubMenuButton"] {
        for i in 0..mask.len() {
            if starts_with_at(mask, i, pattern) {
                let opening = (i..mask.len()).find(|k| mask[*k] == '(');
                if let Some((opening, closing)) = opening.and_then(|o| Some((o, closing(mask, o)?)))
                {
                    zones.push((opening, closing));
                }
            }
        }
    }
    zones
}

/// The `.separator()` placed by hand in a menu, `path:line`. The sections
/// building block (`section_separator`, `menu_sections.rs`) is the only one to
/// place any: its name does not contain "menu", and its tests are test code.
fn raw_separators_in_menu(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        let zones = menu_zones(&source.mask);
        for i in 0..source.mask.len() {
            if starts_with_at(&source.mask, i, ".separator()")
                && zones.iter().any(|(start, end)| *start < i && i < *end)
            {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule of `docs/menus.md`: the rules of a menu are placed **only** through
/// `MenuSections::section`, which puts them only between two present sections.
/// A hand-written `ui.separator()` leaves an orphan rule as soon as a section
/// disappears. No exemption.
#[test]
fn no_raw_separator_in_a_menu() {
    let sources = sources::sources();
    let offenders = raw_separators_in_menu(&sources);
    assert!(
        offenders.is_empty(),
        "a menu places a separator by hand: go through `MenuSections::section` \
         (`app/menu_sections.rs`, `docs/menus.md`): {offenders:?}"
    );
    // The building block exists, and it is what draws the rule.
    let building_block = unique_function(&sources, "section_separator");
    assert!(building_block.body.contains("Separator"));
}

/// The witness: a hand-written rule is seen in a "menu" function, in a
/// right-click closure and in a submenu; a rule outside any menu, a comment and
/// a text do not count.
#[test]
fn the_separators_guard_sees_a_hand_written_rule_in_a_menu() {
    for code in [
        "fn file_menu(ui: &mut Ui) { ui.separator(); }",
        "impl App { fn edit_menu(&self, ui: &mut Ui) { ui.separator(); } }",
        "fn f(r: Response) { r.context_menu(|ui| { ui.button(a); ui.separator(); }); }",
        "fn f(ui: &mut Ui) { ui.menu_button(t, |ui| { ui.separator(); }); }",
    ] {
        assert_eq!(
            raw_separators_in_menu(&[file("app/m.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn bar(ui: &mut Ui) { ui.separator(); }",
        "fn section_separator(ui: &mut Ui) { ui.add(Separator::default()); }",
        "fn f_menu(ui: &mut Ui) { // ui.separator();\n let _ = \"ui.separator()\"; }",
        "fn f_menu() {}\n#[cfg(test)]\nmod tests { fn g_menu(ui: &mut Ui) { ui.separator(); } }",
    ] {
        assert!(
            raw_separators_in_menu(&[file("app/m.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
}

// ---------------------------------------------------------------------------
// Selected button background: reserved for exclusive choices (#169)
// ---------------------------------------------------------------------------

/// The ranges where an egui selectable button is in its place: the menus
/// ([`menu_zones`]), the content of a combo box (arguments of
/// `.show_ui(`) and the body of a popup window function (name that
/// contains "popup" or "picker": the color picker).
fn choice_zones(mask: &[char]) -> Vec<(usize, usize)> {
    let mut zones = menu_zones(mask);
    for (position, name) in functions(mask) {
        let name = short_name(&name);
        if name.contains("popup") || name.contains("picker") {
            zones.extend(body(mask, position));
        }
    }
    for i in 0..mask.len() {
        if starts_with_at(mask, i, ".show_ui(") {
            let opening = i + ".show_ui".len();
            if let Some(end) = closing(mask, opening) {
                zones.push((opening, end));
            }
        }
    }
    zones
}

/// The selectable buttons (`selectable_label(`, `selectable_value(`,
/// `Button::selectable(`) placed outside a menu, a combo box or the color
/// picker, `path:line`.
fn selectable_buttons_outside_choice(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        let zones = choice_zones(&source.mask);
        for i in 0..source.mask.len() {
            let matched = [
                ".selectable_label(",
                ".selectable_value(",
                "Button::selectable(",
            ]
            .iter()
            .any(|pattern| starts_with_at(&source.mask, i, pattern));
            if matched && !zones.iter().any(|(start, end)| *start < i && i < *end) {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule #169 (`docs/application/windows-and-panels.md`, § Tabs, lists and
/// selection): the
/// rounded green background of an egui selected button marks only an
/// **exclusive choice** in a menu, a combo box or the color picker.
/// A list or tree entry is a list row (`list_row`,
/// `ListRowBackground`); a set of tabs, the `tab_strip` band. No
/// exemption.
#[test]
fn no_selectable_button_outside_a_menu() {
    let sources = sources::sources();
    let offenders = selectable_buttons_outside_choice(&sources);
    assert!(
        offenders.is_empty(),
        "selectable button outside a menu or a combo box: a list \
         goes through `list_row` (`app/list_row.rs`), a set of tabs through `tab_strip`: \
         {offenders:?}"
    );
    // The building blocks exist.
    unique_function(&sources, "list_row");
    assert!(unique_function(&sources, "list_row_sensed")
        .body
        .contains("allocate_exact_size"));
    unique_function(&sources, "tab_strip_with");
}

/// The witness: a row of selectable buttons (hand-made tabs)
/// and a list of `selectable_label` are seen; a choice in a combo box, a menu
/// or the color picker passes.
#[test]
fn the_selectable_buttons_guard_sees_a_hand_written_row() {
    for code in [
        "fn page(ui: &mut Ui) { ui.horizontal(|ui| { for a in axes { ui.selectable_label(a == b, t); } }); }",
        "fn list(ui: &mut Ui) { ui.selectable_value(&mut x, 1, t); }",
        "fn row(ui: &mut Ui) { ui.add(egui::Button::selectable(s, t)); }",
    ] {
        assert_eq!(
            selectable_buttons_outside_choice(&[file("app/p.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) { ComboBox::new(a, b).show_ui(ui, |ui| { ui.selectable_value(&mut x, 1, t); }); }",
        "fn type_menu(ui: &mut Ui) { ui.selectable_label(a, t); }",
        "fn f(r: Response) { r.context_menu(|ui| { ui.selectable_label(a, t); }); }",
        "fn picker_contents(ui: &mut Ui) { ui.selectable_value(&mut s, A, t); }",
        "fn f(ui: &mut Ui) { // ui.selectable_label(a, t)\n let _ = \".selectable_label(\"; }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(ui: &mut Ui) { ui.selectable_label(a, t); } }",
    ] {
        assert!(
            selectable_buttons_outside_choice(&[file("app/p.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
}

// ---------------------------------------------------------------------------
// Colors: only through `theme.rs` (#169)
// ---------------------------------------------------------------------------

/// The `Color32` constants that carry no palette choice: white, black,
/// transparent and egui's "no color" witness.
const NEUTRAL_COLORS: [&str; 4] = ["WHITE", "BLACK", "TRANSPARENT", "PLACEHOLDER"];

/// The files that make colors for a living: the palette
/// (`theme.rs`) and the color picker, which paints the colors chosen
/// by the user (gradient, wheel, opacity checkerboard).
const COLOR_FACTORIES: [&str; 2] = ["theme.rs", "color_picker.rs"];

/// The colors hardcoded outside the palette, `path:line`:
/// `Color32::from_…(` and every named `Color32` constant other than
/// [`NEUTRAL_COLORS`].
fn hardcoded_colors(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        if COLOR_FACTORIES.contains(&file.path.as_str()) {
            continue;
        }
        let source = file.production();
        for i in 0..source.mask.len() {
            if !word_at(&source.mask, i, "Color32")
                || !starts_with_at(&source.mask, i + "Color32".len(), "::")
            {
                continue;
            }
            let name = ident_at(&source.mask, i + "Color32::".len());
            let constant = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if name.starts_with("from_") || (constant && !NEUTRAL_COLORS.contains(&name.as_str())) {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule #169: a state or chrome color (accent, hover, rule,
/// error, drop…) comes from a `theme.rs` function or from the `Visuals` that
/// `theme::install` sets (`error_fg_color`, `warn_fg_color`,
/// `selection`…), never from a tint written in a surface: changing the
/// palette is then done in one place. No exemption outside the two
/// factories of [`COLOR_FACTORIES`].
#[test]
fn no_hardcoded_color_outside_the_theme() {
    let offenders = hardcoded_colors(&sources::sources());
    assert!(
        offenders.is_empty(),
        "color hardcoded outside theme.rs: add a palette function \
         in `theme.rs` or read the `Visuals`: {offenders:?}"
    );
}

/// The witness: a `from_rgb`, `from_gray` or named tint is seen, in
/// all its forms; the neutrals, a comment, test code and the
/// two factories pass.
#[test]
fn the_colors_guard_sees_a_hardcoded_tint() {
    for code in [
        "fn f() -> Color32 { egui::Color32::from_rgb(1, 2, 3) }",
        "fn f() -> Color32 { Color32::from_gray(0xEC) }",
        "fn f() -> Color32 { Color32::from_rgba_unmultiplied(1, 2, 3, 4) }",
        "fn f(ui: &Ui) { ui.colored_label(egui::Color32::RED, t); }",
        "fn f() -> Color32 { Color32::LIGHT_BLUE }",
    ] {
        assert_eq!(
            hardcoded_colors(&[file("app/c.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f() -> Color32 { Color32::WHITE }",
        "fn f(c: Color32) -> bool { c == egui::Color32::PLACEHOLDER }",
        "fn f() -> Color32 { theme::accent() } // Color32::from_rgb(1, 2, 3)",
        "fn f() -> MyColor32 { MyColor32::from_rgb(1, 2, 3) }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g() { Color32::from_rgb(1, 2, 3); } }",
    ] {
        assert!(
            hardcoded_colors(&[file("app/c.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
    for path in COLOR_FACTORIES {
        assert!(
            hardcoded_colors(&[file(path, "fn f() { Color32::from_rgb(1, 2, 3); }")]).is_empty()
        );
    }
}

// ---------------------------------------------------------------------------
// Section titles: without a redundant separator (#169)
// ---------------------------------------------------------------------------

/// The `.separator()` directly followed by a section title
/// (`section_title(` or a label in `theme::bold`), `path:line`.
fn separators_before_title(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        let mask = &source.mask;
        for i in 0..mask.len() {
            if !starts_with_at(mask, i, ".separator();") {
                continue;
            }
            let mut k = i + ".separator();".len();
            while k < mask.len() && mask[k].is_whitespace() {
                k += 1;
            }
            // The possible path (`crate::theme::`, `theme::`) before the name.
            let mut name_start = k;
            while name_start < mask.len() && (is_ident(mask[name_start]) || mask[name_start] == ':')
            {
                name_start += 1;
            }
            let call: String = mask[k..name_start].iter().collect();
            let title = short_name(&call) == "section_title"
                || ["section_heading(", "table_header(", "bold("]
                    .iter()
                    .any(|level| {
                        starts_with_at(mask, k, "ui.label(")
                            && ["crate::theme::", "theme::", ""].iter().any(|path| {
                                starts_with_at(
                                    mask,
                                    k + "ui.label(".len(),
                                    &format!("{path}{level}"),
                                )
                            })
                    });
            if title {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule #169: a form section title goes through
/// `theme::section_title` (space + bold) and separates its groups alone; a
/// rule right before it doubles the separation and makes the page heavier
/// than the mockup. No exemption.
#[test]
fn no_separator_before_a_section_title() {
    let sources = sources::sources();
    let offenders = separators_before_title(&sources);
    assert!(
        offenders.is_empty(),
        "separator right before a section title: the title is enough \
         (`theme::section_title`): {offenders:?}"
    );
    assert!(unique_function(&sources, "section_title")
        .body
        .contains("section_heading("));
}

/// The witness: a rule followed by a title, in all its forms, is seen;
/// a rule followed by something else or a lone title pass.
#[test]
fn the_titles_guard_sees_a_rule_before_a_title() {
    for code in [
        "fn f(ui: &mut Ui) { ui.separator();\n    section_title(ui, t); }",
        "fn f(ui: &mut Ui) { ui.separator(); crate::theme::section_title(ui, t); }",
        "fn f(ui: &mut Ui) { ui.separator(); ui.label(crate::theme::bold(t)); }",
        "fn f(ui: &mut Ui) { ui.separator(); ui.label(theme::bold(t)); }",
        "fn f(ui: &mut Ui) { ui.separator(); ui.label(theme::section_heading(t)); }",
        "fn f(ui: &mut Ui) { ui.separator(); ui.label(crate::theme::table_header(t)); }",
    ] {
        assert_eq!(
            separators_before_title(&[file("app/s.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) { section_title(ui, t); }",
        "fn f(ui: &mut Ui) { ui.separator(); ui.label(t); }",
        "fn f(ui: &mut Ui) { ui.separator(); // section_title(ui, t)\n ui.checkbox(a, b); }",
    ] {
        assert!(
            separators_before_title(&[file("app/s.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
}

// ---------------------------------------------------------------------------
// Three text levels: no hand-written bold in small text (#170)
// ---------------------------------------------------------------------------

/// The `bold(…)` whose method chain contains `.small()`, outside
/// `theme.rs` (which defines the levels), `path:line`.
fn bold_in_small_text(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        if file.path == "theme.rs" {
            continue;
        }
        let source = file.production();
        let mask = &source.mask;
        for i in 0..mask.len() {
            if !word_at(mask, i, "bold") || mask.get(i + 4) != Some(&'(') {
                continue;
            }
            let Some(mut k) = closing(mask, i + 4) else {
                continue;
            };
            k += 1;
            // The `.method(…)` chain that follows the call.
            loop {
                while k < mask.len() && mask[k].is_whitespace() {
                    k += 1;
                }
                if mask.get(k) != Some(&'.') {
                    break;
                }
                k += 1;
                while k < mask.len() && mask[k].is_whitespace() {
                    k += 1;
                }
                let name = ident_at(mask, k);
                if name.is_empty() {
                    break;
                }
                if name == "small" {
                    found.push(format!(
                        "src/{}:{}",
                        file.path,
                        line_of(&source.original, i)
                    ));
                    break;
                }
                k += name.chars().count();
                if mask.get(k) != Some(&'(') {
                    break;
                }
                match closing(mask, k) {
                    Some(end) => k = end + 1,
                    None => break,
                }
            }
        }
    }
    found
}

/// Rule #170 (point 10): a bold in small text is the "table header" level
/// (`theme::table_header`), never a `bold(…).small()` written in
/// place. No exemption.
#[test]
fn no_small_bold_outside_the_theme() {
    let offenders = bold_in_small_text(&sources::sources());
    assert!(
        offenders.is_empty(),
        "`bold(…).small()` outside theme.rs — use theme::table_header          (table header) or theme::section_heading (title): {offenders:?}"
    );
}

/// The witness: `bold(…).small()`, alone or followed by other methods, is seen;
/// bold alone, small text alone, `theme.rs` and test code pass.
#[test]
fn the_small_bold_guard_sees_a_bold_small() {
    for code in [
        "fn f(ui: &mut Ui) { ui.label(crate::theme::bold(t).small()); }",
        "fn f(ui: &mut Ui) { ui.label(theme::bold(t).color(c).small()); }",
        "fn f(ui: &mut Ui) { ui.label(bold(format!(\"{}\", (a))).small()); }",
    ] {
        assert_eq!(
            bold_in_small_text(&[file("app/g.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) { ui.label(theme::bold(t)); }",
        "fn f(ui: &mut Ui) { ui.label(RichText::new(t).small()); }",
        "fn f(ui: &mut Ui) { ui.label(theme::bold(t).color(c)); ui.label(x.small()); }",
        "fn f() {}
#[cfg(test)]
mod tests { fn g() { bold(t).small(); } }",
        "fn f(ui: &mut Ui) { ui.label(my_bold(t).small()); }",
    ] {
        assert!(
            bold_in_small_text(&[file("app/g.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
    assert!(bold_in_small_text(&[file(
        "theme.rs",
        "fn table_header(t: &str) -> RichText { bold(t).small() }"
    )])
    .is_empty());
}

// ---------------------------------------------------------------------------
// No `DragValue` nor `Slider`: the "number" building block (#220)
// ---------------------------------------------------------------------------

/// The egui numeric fields (`DragValue`, `Slider`) written in the production
/// code, `path:line`. No file is exempt: the building block
/// (`number_field.rs`) is itself a text field.
fn egui_numeric_fields(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        for i in 0..source.mask.len() {
            if word_at(&source.mask, i, "DragValue") || word_at(&source.mask, i, "Slider") {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule #220 (`docs/interface-standard.md`, § Numeric fields): a number
/// is typed through `number_field::number_field`, with its quantity from
/// `quantities.rs`; bounds, step and decimals are not written in place.
#[test]
fn no_numeric_field_outside_the_building_block() {
    let sources = sources::sources();
    let offenders = egui_numeric_fields(&sources);
    assert!(
        offenders.is_empty(),
        "egui `DragValue`/`Slider`: use `number_field::number_field` and a quantity of \
         `quantities.rs`: {offenders:?}"
    );
    // The building block exists.
    assert!(unique_function(&sources, "number_field_core")
        .body
        .contains("edit_text"));
}

/// The witness: a `DragValue` or a `Slider`, in any form, is seen; a
/// comment, a string, a name that contains them and test code pass.
#[test]
fn the_numeric_fields_guard_sees_a_drag_value() {
    for code in [
        "fn f(ui: &mut Ui) { ui.add(egui::DragValue::new(&mut x)); }",
        "fn f(ui: &mut Ui) { ui.add(DragValue::new(&mut x).range(0..=9)); }",
        "fn f(ui: &mut Ui) { ui.add(egui::Slider::new(&mut x, 0.0..=1.0)); }",
    ] {
        assert_eq!(
            egui_numeric_fields(&[file("app/n.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f() {} // egui::DragValue::new(&mut x)",
        "fn f() -> &'static str { \"DragValue\" }",
        "fn f(x: MySlider) {}",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(ui: &mut Ui) { ui.add(DragValue::new(&mut x)); } }",
    ] {
        assert!(
            egui_numeric_fields(&[file("app/n.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
}

// ---------------------------------------------------------------------------
// No bare tooltip on a form field (#225, S-M2)
// ---------------------------------------------------------------------------

/// The constructions that designate a form field in the chain of a
/// tooltip: checkbox, radio button, text field, number field, and the
/// translated label of a field (`label(tr!(…))`).
const FIELDS: [&str; 6] = [
    ".checkbox(",
    ".radio_value(",
    "text_edit_singleline",
    "TextEdit::",
    "number_field",
    ".label(tr!(",
];

/// The start of the statement that contains `position`: after the last
/// top-level `;`, `{` or `}` before it.
fn statement_start(mask: &[char], position: usize) -> usize {
    let mut depth = 0usize;
    for k in (0..position).rev() {
        match mask[k] {
            ')' | ']' => depth += 1,
            '(' | '[' => {
                if depth == 0 {
                    return k + 1;
                }
                depth -= 1;
            }
            ';' | '{' | '}' if depth == 0 => return k + 1,
            _ => {}
        }
    }
    0
}

/// The tooltips (`on_hover_text`, `on_hover_ui`) placed on a form field
/// outside a menu, `path:line`. A tooltip on a button, an icon, a tab
/// or a menu is still allowed (S-M1); the reminder of a field goes through the
/// "?" icon (`hover_help`).
fn tooltips_on_field(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        let mut zones = menu_zones(&source.mask);
        // The content of a menu painted by a shared function (`…_contents`,
        // `docs/menus.md`) is a menu.
        for (position, name) in functions(&source.mask) {
            if short_name(&name).ends_with("_contents") {
                zones.extend(body(&source.mask, position));
            }
        }
        for i in 0..source.mask.len() {
            if !(starts_with_at(&source.mask, i, ".on_hover_text(")
                || starts_with_at(&source.mask, i, ".on_hover_ui("))
                || zones.iter().any(|(start, end)| *start < i && i < *end)
            {
                continue;
            }
            let start = statement_start(&source.mask, i);
            let chain: String = source.mask[start..i].iter().collect();
            if FIELDS.iter().any(|field| chain.contains(field)) {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule #225 (`docs/interface-standard.md`, § Messages and help, S-M2): a
/// form field never has a bare tooltip; its reminder is the "?" icon
/// (`app/hover_help.rs`), its effect or scope a plain text.
#[test]
fn no_bare_tooltip_on_a_form_field() {
    let sources = sources::sources();
    let offenders = tooltips_on_field(&sources);
    assert!(
        offenders.is_empty(),
        "bare tooltip on a form field: place the \"?\" icon (`hover_help`, \
         `help_beside`) or a plain text: {offenders:?}"
    );
    unique_function(&sources, "hover_help");
}

/// The witness: a tooltip on a checkbox, a radio, a text field, a number
/// field or a field label is seen; a tooltip on a button, an icon or a
/// menu, a comment and test code pass.
#[test]
fn the_tooltips_guard_sees_a_tooltip_on_a_field() {
    for code in [
        "fn f(ui: &mut Ui) { ui.checkbox(&mut a, t).on_hover_text(h); }",
        "fn f(ui: &mut Ui) { ui.radio_value(&mut a, 1, t)\n.on_hover_text(h); }",
        "fn f(ui: &mut Ui) { ui.add(TextEdit::singleline(&mut s)).on_hover_text(h); }",
        "fn f(ui: &mut Ui) { ui.text_edit_singleline(&mut s).on_hover_ui(|ui| {}); }",
        "fn f(ui: &mut Ui) { number_field(ui, &mut x).response.on_hover_text(h); }",
        "fn f(ui: &mut Ui) { ui.label(tr!(\"key\")).on_hover_text(h); }",
        "fn f(ui: &mut Ui) { if ui.checkbox(&mut a, t).on_hover_text(h).clicked() {} }",
    ] {
        assert_eq!(
            tooltips_on_field(&[file("app/b.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) { ui.button(t).on_hover_text(h); }",
        "fn f(ui: &mut Ui) { ui.checkbox(&mut a, t); ui.button(t).on_hover_text(h); }",
        "fn f(ui: &mut Ui) { ui.add(icon_image(i)).on_hover_text(h); }",
        "fn f(ui: &mut Ui) { ui.label(shown).on_hover_text(exact); }",
        "fn view_menu(ui: &mut Ui) { ui.checkbox(&mut a, t).on_hover_text(h); }",
        "fn settings_contents(ui: &mut Ui) { ui.checkbox(&mut a, t).on_hover_text(h); }",
        "fn f(ui: &mut Ui) { // ui.checkbox(a, t).on_hover_text(h)\n }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(ui: &mut Ui) { ui.checkbox(a, t).on_hover_text(h); } }",
    ] {
        assert!(
            tooltips_on_field(&[file("app/b.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
}

/// The `.add_enabled(…)` calls whose method chain does not carry
/// `.on_disabled_hover_text(…)`, `path:line` (#226, S-G1/S-G2). An
/// `add_enabled_ui`, which disables a group, is not read: the reason is placed
/// on the element, never on the group.
fn disabled_without_reason(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        let mask = &source.mask;
        for i in 1..mask.len() {
            if mask[i - 1] != '.' || !starts_with_at(mask, i, "add_enabled(") {
                continue;
            }
            let Some(mut j) = closing(mask, i + "add_enabled".len()).map(|k| k + 1) else {
                continue;
            };
            let mut reason = false;
            loop {
                while mask.get(j).is_some_and(|c| c.is_whitespace()) {
                    j += 1;
                }
                if mask.get(j) != Some(&'.') {
                    break;
                }
                let name = ident_at(mask, j + 1);
                j += 1 + name.chars().count();
                if name == "on_disabled_hover_text" {
                    reason = true;
                }
                if mask.get(j) == Some(&'(') {
                    match closing(mask, j) {
                        Some(k) => j = k + 1,
                        None => break,
                    }
                }
            }
            if !reason {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule #226 (`docs/interface-standard.md`, § Disabled, S-G1/S-G2): an element
/// disabled through `add_enabled` says why and what to do, in the
/// `.on_disabled_hover_text` tooltip of the element itself.
#[test]
fn no_disabled_element_without_its_reason() {
    let offenders = disabled_without_reason(&sources::sources());
    assert!(
        offenders.is_empty(),
        "`add_enabled` without `.on_disabled_hover_text(tr!(…))`: say why the element is \
         disabled and what to do (`disabled-*` keys): {offenders:?}"
    );
}

/// The witness: an `add_enabled` without a reason, in any form, is seen; with
/// its reason (placed anywhere in the chain), an `add_enabled_ui`, a
/// comment and test code pass.
#[test]
fn the_disabling_guard_sees_an_add_enabled_without_reason() {
    for code in [
        "fn f(ui: &mut Ui) { ui.add_enabled(ok, Button::new(t)); }",
        "fn f(ui: &mut Ui) { if ui.add_enabled(ok, Button::new(t)).clicked() {} }",
        "fn f(ui: &mut Ui) { ui\n.add_enabled(ok, Button::new(t))\n.on_hover_text(h)\n.clicked(); }",
    ] {
        assert_eq!(
            disabled_without_reason(&[file("app/g.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) { ui.add_enabled(ok, Button::new(t)).on_disabled_hover_text(r); }",
        "fn f(ui: &mut Ui) { if ui.add_enabled(ok, b).on_hover_text(h).on_disabled_hover_text(tr!(\"k\")).clicked() {} }",
        "fn f(ui: &mut Ui) { ui.add_enabled_ui(ok, |ui| { ui.label(t); }); }",
        "fn f(ui: &mut Ui) { // ui.add_enabled(ok, b);\n }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(ui: &mut Ui) { ui.add_enabled(ok, b); } }",
    ] {
        assert!(
            disabled_without_reason(&[file("app/g.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
}

// ---------------------------------------------------------------------------
// No literal `add_space` outside `theme.rs` (#227, S-L4)
// ---------------------------------------------------------------------------

/// The `add_space(<number>)` of the production code outside `theme.rs`,
/// `path:line`. A computed gap (`add_space(indent)`, a difference of
/// widths) passes: only a number written in place is an invented
/// spacing.
fn literal_add_space(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        if file.path == "theme.rs" {
            continue;
        }
        let source = file.production();
        for i in 0..source.mask.len() {
            if !starts_with_at(&source.mask, i, "add_space(") {
                continue;
            }
            let argument: String = source.mask[i + "add_space(".len()..]
                .iter()
                .take_while(|c| **c != ')')
                .filter(|c| !c.is_whitespace())
                .collect();
            if argument
                .trim_start_matches('-')
                .starts_with(|c: char| c.is_ascii_digit() || c == '.')
            {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// The `consume_key(` and `KeyboardShortcut::new(` of the production code outside
/// `app/shortcuts.rs`, `path:line` (S-K1, #231).
fn keys_outside_registry(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        if file.path == "app/shortcuts.rs" {
            continue;
        }
        let source = file.production();
        for i in 0..source.mask.len() {
            if starts_with_at(&source.mask, i, "consume_key(")
                || starts_with_at(&source.mask, i, "KeyboardShortcut::new(")
            {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule #231 (`docs/interface-standard.md`, § Keyboard and mouse, S-K1): a
/// single registry of keys, `app/shortcuts.rs`.
#[test]
fn no_consume_key_outside_the_registry() {
    let sources = sources::sources();
    let offenders = keys_outside_registry(&sources);
    assert!(
        offenders.is_empty(),
        "every consumed key is a constant of `app/shortcuts.rs` \
         (`input.consume_shortcut(&KEY_…)`): {offenders:?}"
    );
}

/// The witness: both forms are seen outside the registry; the registry, a
/// comment and test code pass.
#[test]
fn the_key_registry_guard_sees_a_key_written_in_place() {
    for code in [
        "fn f(i: &mut InputState) { i.consume_key(Modifiers::NONE, Key::Delete); }",
        "const S: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F5);",
    ] {
        assert_eq!(
            keys_outside_registry(&[file("app/e.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f(i: &mut InputState) { i.consume_shortcut(&KEY_DELETE); }",
        "fn f() {} // i.consume_key(Modifiers::NONE, Key::Delete)",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(i: &mut InputState) { i.consume_key(M, K); } }",
    ] {
        assert!(
            keys_outside_registry(&[file("app/e.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
    assert!(keys_outside_registry(&[file(
        "app/shortcuts.rs",
        "const S: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F5);"
    )])
    .is_empty());
}

/// The `CursorIcon::` of the production code outside `app/cursors.rs`,
/// `path:line` (S-X2, #231). The type alone (`egui::CursorIcon,`) passes.
fn cursors_outside_module(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        if file.path == "app/cursors.rs" {
            continue;
        }
        let source = file.production();
        for i in 0..source.mask.len() {
            if starts_with_at(&source.mask, i, "CursorIcon::") {
                found.push(format!(
                    "src/{}:{}",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    found
}

/// Rule #231 (`docs/interface-standard.md`, § Keyboard and mouse, S-X2): a
/// single module names the cursors.
#[test]
fn no_cursor_outside_the_cursors_module() {
    let sources = sources::sources();
    let offenders = cursors_outside_module(&sources);
    assert!(
        offenders.is_empty(),
        "a cursor is chosen through `app/cursors.rs` (`grab`, `clickable`, `RESIZE_*`): {offenders:?}"
    );
}

/// The witness: a cursor written in place is seen; the module, the type alone,
/// a comment and test code pass.
#[test]
fn the_cursors_guard_sees_a_cursor_written_in_place() {
    assert_eq!(
        cursors_outside_module(&[file(
            "app/e.rs",
            "fn f(c: &Context) { c.set_cursor_icon(egui::CursorIcon::Grab); }"
        )])
        .len(),
        1
    );
    for code in [
        "fn f(z: Vec<(egui::CursorIcon, u8)>) {}",
        "fn f() {} // egui::CursorIcon::Grab",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g() { let _ = egui::CursorIcon::Grab; } }",
    ] {
        assert!(
            cursors_outside_module(&[file("app/e.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
    assert!(cursors_outside_module(&[file(
        "app/cursors.rs",
        "const R: egui::CursorIcon = egui::CursorIcon::ResizeNorth;"
    )])
    .is_empty());
}

/// Rule #227 (`docs/interface-standard.md`, § Choices, layout, S-L4): two
/// spacing constants in `theme.rs`, no numeric `add_space`
/// elsewhere.
#[test]
fn no_literal_add_space_outside_the_theme() {
    let sources = sources::sources();
    let offenders = literal_add_space(&sources);
    assert!(
        offenders.is_empty(),
        "`add_space(<number>)`: use `theme::SPACE_ITEM` or `theme::SPACE_SECTION`: \
         {offenders:?}"
    );
}

/// The witness: a number, integer or decimal, is seen; a constant, a computed
/// gap, `theme.rs`, a comment and test code pass.
#[test]
fn the_spacing_guard_sees_a_number_written_in_place() {
    for code in [
        "fn f(ui: &mut Ui) { ui.add_space(6.0); }",
        "fn f(ui: &mut Ui) { ui.add_space( 12 ); }",
        "fn f(ui: &mut Ui) { ui.add_space(.5); }",
        "fn f(ui: &mut Ui) { ui.add_space(-2.0); }",
    ] {
        assert_eq!(
            literal_add_space(&[file("app/e.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) { ui.add_space(theme::SPACE_ITEM); }",
        "fn f(ui: &mut Ui) { ui.add_space(width - text); }",
        "fn f(ui: &mut Ui) { ui.add_space(ui.spacing().item_spacing.y); }",
        "fn f(ui: &mut Ui) {} // ui.add_space(6.0)",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(ui: &mut Ui) { ui.add_space(80.0); } }",
    ] {
        assert!(
            literal_add_space(&[file("app/e.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
    assert!(
        literal_add_space(&[file("theme.rs", "fn f(ui: &mut Ui) { ui.add_space(6.0); }")])
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Tests do not write to the real preferences (#250)
// ---------------------------------------------------------------------------

/// `AppState::new` loads `~/.config/stokesforge/preferences.json`: an
/// integration test goes through `new_with_preferences`. The integration tests
/// of the state live in `stokesforge-app-state/tests/` (#263): both `tests/`
/// directories are read. (The unit tests of `src/`, for their part, build their
/// `AppState` through `new`: `tests/preferences_isolation.rs` holds that
/// this does not reach the real file.) Only `preferences_isolation.rs` calls
/// `AppState::new`: it is the test that proves the call is harmless,
/// `STOKESFORGE_CONFIG_DIR` redirected to a temporary directory.
#[test]
fn no_integration_test_uses_appstate_new() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dirs = [
        manifest.join("tests"),
        manifest.join("../stokesforge-app-state/tests"),
    ];
    let mut offenders = Vec::new();
    for dir in dirs {
        for entry in std::fs::read_dir(&dir).expect("tests directory") {
            let path = entry.expect("entry").path();
            if path.extension().is_some_and(|e| e == "rs")
                && path
                    .file_name()
                    .is_some_and(|n| n != "guards.rs" && n != "preferences_isolation.rs")
                && std::fs::read_to_string(&path)
                    .expect("read")
                    .contains("AppState::new(")
            {
                offenders.push(path.display().to_string());
            }
        }
    }
    assert!(offenders.is_empty(), "AppState::new in {offenders:?}");
}

// ---------------------------------------------------------------------------
// A single native file picker (#192)
// ---------------------------------------------------------------------------

/// The only file where `rfd::` may be written: the path of `file_picker.rs`
/// in the list of sources (state crate, #263).
const PICKER_MODULE: &str = "app-state/file_picker.rs";

/// The `rfd::` written outside [`PICKER_MODULE`] in the code (comments
/// excluded, tests included), `path:line`.
fn rfd_pickers_outside_module(sources: &[SourceFile]) -> Vec<String> {
    sources
        .iter()
        .filter(|file| file.path != PICKER_MODULE)
        .flat_map(|file| {
            file.code()
                .lines()
                .enumerate()
                .filter(|(_, line)| line.contains("rfd::"))
                .map(|(index, _)| format!("src/{}:{}", file.path, index + 1))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Guard (#192): an `rfd::FileDialog` called outside `file_picker.rs` would be
/// synchronous on the interface thread — the portal freeze.
#[test]
fn no_rfd_picker_outside_the_module() {
    let sources = sources::sources();
    assert!(
        sources.iter().any(|f| f.path == PICKER_MODULE),
        "{PICKER_MODULE} not read: the guard would no longer see the module"
    );
    let offenders = rfd_pickers_outside_module(&sources);
    assert!(
        offenders.is_empty(),
        "file picker outside file_picker.rs — use \
         Action::PickFile(PickRequest): {offenders:?}"
    );
}

/// The witness: an `rfd::` written in a surface is seen; the picker module
/// (at the path of the real list) and a comment pass.
#[test]
fn the_picker_guard_sees_an_rfd_outside_the_module() {
    let code = "fn f() { let _ = rfd::FileDialog::new(); }";
    assert_eq!(
        rfd_pickers_outside_module(&[file("app/menus.rs", code)]).len(),
        1
    );
    assert_eq!(
        rfd_pickers_outside_module(&[file("app-state/state.rs", code)]).len(),
        1
    );
    assert!(rfd_pickers_outside_module(&[file(PICKER_MODULE, code)]).is_empty());
    assert!(
        rfd_pickers_outside_module(&[file("app/menus.rs", "fn f() {} // rfd::FileDialog")])
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// No tool picker outside the Tools bar (#299, D1, D5)
// ---------------------------------------------------------------------------

/// The exception marker of a tool picker placed in a window:
/// `// TOOL-IN-WINDOW(#299): <why this window departs>`.
const TOOL_MARKER: &str = "TOOL-IN-WINDOW(#299):";

/// The lines before the call where the marker is looked for.
const MARKER_SCOPE: usize = 12;

/// Minimum length (in characters) of a justification: "ok" is not one.
const MINIMUM_JUSTIFICATION: usize = 15;

/// The forms that paint an exclusive choice or a toggle: egui's
/// selectable buttons and radios (the `choice` building block is seen separately).
const CHOICE_FORMS: [&str; 7] = [
    ".selectable_label(",
    ".selectable_value(",
    "Button::selectable(",
    "RadioButton::new(",
    ".radio(",
    ".radio_value(",
    ".toggle_value(",
];

/// The position of the opening parenthesis of a choice written at `i`, if there
/// is one: the form's `choice` / `choice_by` building block or an egui form.
fn choice_opening(mask: &[char], i: usize) -> Option<usize> {
    for building_block in ["choice", "choice_by", "toggle_texts"] {
        if word_at(mask, i, building_block) && mask.get(i + building_block.len()) == Some(&'(') {
            return Some(i + building_block.len());
        }
    }
    CHOICE_FORMS
        .iter()
        .find(|form| starts_with_at(mask, i, form))
        .map(|form| i + form.len() - 1)
}

/// Do the arguments of a choice name a tool (`form.tool`, `PreviewTool`,
/// `tr!("analysis-tool-…")`)? "toolbar" and "tooltip" do not.
fn names_a_tool(arguments: &str) -> bool {
    arguments
        .to_lowercase()
        .replace("toolbar", "")
        .replace("tooltip", "")
        .contains("tool")
}

/// The justification of `marker` (here [`TOOL_MARKER`], further down
/// [`SIZE_MARKER`]) in the [`MARKER_SCOPE`] lines before the line `line`
/// (1-based) or on it, `None` if there is no marker.
fn marker_justification(original: &[char], line: usize, marker: &str) -> Option<String> {
    let text: String = original.iter().collect();
    let lines: Vec<&str> = text.lines().collect();
    let end = line.min(lines.len());
    let start = end.saturating_sub(MARKER_SCOPE + 1);
    lines[start..end].iter().rev().find_map(|line| {
        let (_, rest) = line.split_once(marker)?;
        Some(rest.trim().to_string())
    })
}

/// The tool pickers of the production code outside `toolbar.rs` (the Tools
/// bar, the only home of tools) that have no justified exception,
/// `path:line: reason`.
fn tool_pickers_without_exception(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        if file.path == "toolbar.rs" {
            continue;
        }
        let source = file.production();
        for i in 0..source.mask.len() {
            let Some(opening) = choice_opening(&source.mask, i) else {
                continue;
            };
            let Some(end) = closing(&source.mask, opening) else {
                continue;
            };
            let arguments: String = source.code[opening..=end].iter().collect();
            if !names_a_tool(&arguments) {
                continue;
            }
            let line = line_of(&source.original, i);
            let place = format!("src/{}:{line}", file.path);
            match marker_justification(&source.original, line, TOOL_MARKER) {
                None => found.push(format!("{place}: no marker")),
                Some(text) if text.chars().count() < MINIMUM_JUSTIFICATION => {
                    found.push(format!("{place}: empty or too short justification"));
                }
                Some(_) => {}
            }
        }
    }
    found
}

/// Rule #299 (D1, D5): a tool picker (an exclusive choice whose
/// arguments name a tool) is not in a window; tools stay
/// in the Tools bar. One may leave it — the analysis tools, for example —
/// provided it is written in the code: `// TOOL-IN-WINDOW(#299): <why>`
/// in the twelve lines before, non-empty justification. Gesture matrix:
/// `docs/gestures.md`.
#[test]
fn no_tool_picker_outside_the_tools_bar() {
    let sources = sources::sources();
    assert!(
        sources.iter().any(|f| f.path == "toolbar.rs"),
        "toolbar.rs not read: the guard would no longer see the Tools bar"
    );
    let offenders = tool_pickers_without_exception(&sources);
    assert!(
        offenders.is_empty(),
        "tool picker outside the Tools bar — place it in the bar, or \
         write `// {TOOL_MARKER} <justification>` right before (docs/gestures.md): \
         {offenders:?}"
    );
}

/// The witness: a bare tool picker is seen, in each choice form; an
/// exception without justification or without the right marker is seen too; a
/// justified exception, the Tools bar, a choice of another nature, a comment
/// and test code pass.
#[test]
fn the_tools_guard_sees_a_bare_tool_picker() {
    for code in [
        "fn f(ui: &mut Ui, form: &mut F) { choice(ui, id, &mut form.tool, &[]); }",
        "fn f(ui: &mut Ui) { choice_by(ui, id, &mut t, &[Opt::new(Tool::Zoom, tr!(\"x\"))]); }",
        "fn f(ui: &mut Ui) { ui.selectable_value(&mut tool, Tool::Zone, t); }",
        "fn f(ui: &mut Ui) { ui.radio_value(&mut mode, 1, tr!(\"analysis-tool-zone\")); }",
        "fn f(ui: &mut Ui) { ui.add(egui::RadioButton::new(tool == Pointer, \"tool\")); }",
        "fn f(ui: &mut Ui) { ui.selectable_label(tool == Tool::Zoom, t); }",
        "fn f(ui: &mut Ui, form: &mut F) { toggle_texts(ui, id, &mut form.tool, &[]); }",
    ] {
        assert_eq!(
            tool_pickers_without_exception(&[file("app/w.rs", code)]).len(),
            1,
            "not seen: {code}"
        );
    }
    // Badly written exception: empty, too short, other marker, too far.
    let far = "\n".repeat(MARKER_SCOPE + 2);
    for before in [
        "// TOOL-IN-WINDOW(#299):\n".to_string(),
        "// TOOL-IN-WINDOW(#299): ok\n".to_string(),
        "// TOOL-IN-WINDOW: the analysis tools are numerous\n".to_string(),
        "// TOOL-IN-WINDOW(#298): the analysis tools are numerous\n".to_string(),
        format!("// TOOL-IN-WINDOW(#299): the analysis tools are numerous{far}"),
    ] {
        let code =
            format!("fn f(ui: &mut Ui) {{\n{before}choice(ui, id, &mut form.tool, &[]);\n}}");
        assert_eq!(
            tool_pickers_without_exception(&[file("app/w.rs", &code)]).len(),
            1,
            "exception wrongly accepted: {code}"
        );
    }
    for code in [
        "fn f(ui: &mut Ui) {\n// TOOL-IN-WINDOW(#299): the analysis tools need gestures of their own\nchoice(ui, id, &mut form.tool, &[]);\n}",
        "fn f(ui: &mut Ui) {\n// TOOL-IN-WINDOW(#299): the analysis tools need gestures of their own\n// (rest of the justification)\nui.radio_value(&mut tool, 1, t);\n}",
        "fn f(ui: &mut Ui) { choice(ui, id, &mut form.layout, &[]); }",
        "fn f(ui: &mut Ui) { ui.selectable_label(a, tr!(\"toolbar-text\")); }",
        "fn f(ui: &mut Ui) { ui.selectable_label(a, t).on_hover_text(tooltip); }",
        "fn f(ui: &mut Ui) { // choice(ui, id, &mut form.tool, &[]);\n let _ = \"ui.radio_value(tool)\"; }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(ui: &mut Ui) { choice(ui, id, &mut tool, &[]); } }",
    ] {
        assert!(
            tool_pickers_without_exception(&[file("app/w.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
    let bar = "fn f(ui: &mut Ui) { ui.selectable_value(&mut tool, Tool::Zone, t); }";
    assert!(tool_pickers_without_exception(&[file("toolbar.rs", bar)]).is_empty());
}

// ---------------------------------------------------------------------------
// Every wheel or double-click gesture is in the matrix (#299, D4)
// ---------------------------------------------------------------------------

/// The prefix of the `stokesforge-render-egui` paths in the list of sources
/// read by the gestures guard: `render-egui/lib.rs`.
const RENDER_EGUI_PREFIX: &str = "render-egui/";

/// What reads a wheel or a double click: the gestures whose meaning
/// must be the same from one surface to the next (Ctrl + wheel, double click).
const GESTURE_READS: [&str; 4] = ["scroll_delta", "zoom_delta", "MouseWheel", "double_clicked"];

/// The gesture matrix.
fn gesture_matrix() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("gestures.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} unreadable: {e}", path.display()))
}

/// The sources of the application **and** of the egui rendering (the graph lives there).
fn gesture_sources() -> Vec<SourceFile> {
    let render = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("stokesforge-render-egui")
        .join("src");
    let mut tout = sources::sources();
    tout.extend(sources::read_dirs(&[(
        render.as_path(),
        RENDER_EGUI_PREFIX,
    )]));
    tout
}

/// The `….rs` paths cited between backticks in `matrix`.
fn cited_files(matrix: &str) -> Vec<String> {
    matrix
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| {
            token.ends_with(".rs") && !token.contains(' ') && !token.starts_with("crates/")
        })
        .map(str::to_string)
        .collect()
}

/// Deviations between the code and the matrix: a production file that reads a
/// wheel or a double click without being cited, a cited file that no longer exists.
fn matrix_deviations(sources: &[SourceFile], matrix: &str) -> Vec<String> {
    let cited = cited_files(matrix);
    let mut deviations = Vec::new();
    for file in sources {
        let source = file.production();
        let lit = (0..source.mask.len()).find(|&i| {
            GESTURE_READS
                .iter()
                .any(|pattern| starts_with_at(&source.mask, i, pattern))
        });
        if let Some(i) = lit {
            if !cited.contains(&file.path) {
                deviations.push(format!(
                    "src/{}:{} reads a gesture absent from docs/gestures.md",
                    file.path,
                    line_of(&source.original, i)
                ));
            }
        }
    }
    for citation in cited {
        if !sources.iter().any(|f| f.path == citation) {
            deviations.push(format!(
                "`{citation}` cited by docs/gestures.md does not exist"
            ));
        }
    }
    deviations
}

/// Rule #299 (D4): the `docs/gestures.md` matrix names each surface that reads
/// a wheel or a double click. A new surface, or a new wheel gesture
/// in a file that read none, is entered in the matrix — where
/// one sees whether the same gesture does something else elsewhere. The guard does not judge the
/// gesture; it prevents it from being born outside the page.
#[test]
fn no_gesture_outside_the_matrix() {
    let sources = gesture_sources();
    assert!(
        sources.iter().any(|f| f.path == "render-egui/lib.rs"),
        "render-egui/lib.rs not read: the guard would no longer see the graph"
    );
    let deviations = matrix_deviations(&sources, &gesture_matrix());
    assert!(deviations.is_empty(), "{deviations:?}");
}

/// The witness: a wheel or a double click in an uncited file is seen,
/// in each form; a cited file, a comment, a string and test code
/// pass; a stale citation is seen.
#[test]
fn the_gestures_guard_sees_a_gesture_outside_the_matrix() {
    let matrix = "Surfaces: `app/grid.rs`, `app/old.rs`.";
    let existing = |code: &str, path: &str| {
        let mut all = vec![file(path, code), file("app/old.rs", "fn v() {}")];
        if path != "app/grid.rs" {
            all.push(file("app/grid.rs", "fn g() {}"));
        }
        all
    };
    for code in [
        "fn f(ui: &mut Ui) { let s = ui.input(|i| i.smooth_scroll_delta.y); }",
        "fn f(c: &Context) { let z = c.input(|i| i.zoom_delta()); }",
        "fn f(e: Event) { if let Event::MouseWheel { .. } = e {} }",
        "fn f(r: Response) { if r.double_clicked() {} }",
    ] {
        assert_eq!(
            matrix_deviations(&existing(code, "app/new.rs"), matrix).len(),
            1,
            "not seen: {code}"
        );
        // Cited in the matrix: passes.
        assert!(
            matrix_deviations(&existing(code, "app/grid.rs"), matrix).is_empty(),
            "false positive: {code}"
        );
    }
    for code in [
        "fn f() { // r.double_clicked()\n let _ = \"scroll_delta\"; }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g(r: Response) { r.double_clicked(); } }",
    ] {
        assert!(
            matrix_deviations(&existing(code, "app/new.rs"), matrix).is_empty(),
            "false positive: {code}"
        );
    }
    // A stale citation: `app/old.rs` no longer exists.
    let deviations = matrix_deviations(&[file("app/grid.rs", "fn g() {}")], matrix);
    assert_eq!(deviations.len(), 1, "{deviations:?}");
}

// ---------------------------------------------------------------------------
// A free size is justified (#300, S-F11)
// ---------------------------------------------------------------------------

/// The marker of an interactive window outside the size classes:
/// `// FREE-SIZE(#300): <why no class fits>`.
const SIZE_MARKER: &str = "FREE-SIZE(#300):";

/// The form that asks for a free size: the window **without** rich interaction
/// takes the size of its content through `PlainSize::Content`, without a marker.
const FREE_SIZE_CALL: &str = "InteractiveSize::Free(";

/// The free sizes of the production code without their justified exception,
/// `path:line: reason`.
fn free_sizes_without_justification(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        let source = file.production();
        for i in 0..source.mask.len() {
            if !starts_with_at(&source.mask, i, FREE_SIZE_CALL) {
                continue;
            }
            let line = line_of(&source.original, i);
            let place = format!("src/{}:{line}", file.path);
            match marker_justification(&source.original, line, SIZE_MARKER) {
                None => found.push(format!("{place}: no marker")),
                Some(text) if text.chars().count() < MINIMUM_JUSTIFICATION => {
                    found.push(format!("{place}: empty or too short justification"));
                }
                Some(_) => {}
            }
        }
    }
    found
}

/// Rule #300 (S-F11): an interactive window (settings, draft,
/// wizard) takes one of the three size classes of `app/dialogs.rs`
/// (Small, Medium, Large); outside the classes, `InteractiveSize::Free` requires
/// `// FREE-SIZE(#300): <justification>` in the twelve lines
/// before, justification of at least 15 characters. Windows without
/// rich interaction (confirmation, alert, read-only, choice) take the
/// size of their content without a marker: the type does not let the
/// interactive window do the same.
#[test]
fn no_free_size_without_justification() {
    let sources = sources::sources();
    assert!(
        sources.iter().any(|f| f.path == "app/dialogs.rs"),
        "app/dialogs.rs not read: the guard would no longer see the dialogs building block"
    );
    let offenders = free_sizes_without_justification(&sources);
    assert!(
        offenders.is_empty(),
        "free size without justification — take a class (SizeClass) or write \
         `// {SIZE_MARKER} <justification>` right before (docs/windows.md): {offenders:?}"
    );
}

/// The witness: a bare free size is seen; an empty or too short exception,
/// with another number, without a number or too far is seen too; a
/// justified exception, a comment, a string and test code pass.
#[test]
fn the_sizes_guard_sees_a_bare_free_size() {
    let bare = "fn f() { let _ = DialogNature::Draft(InteractiveSize::Free(vec2(1.0, 2.0))); }";
    assert_eq!(
        free_sizes_without_justification(&[file("app/w.rs", bare)]).len(),
        1
    );
    let far = "\n".repeat(MARKER_SCOPE + 2);
    for before in [
        "// FREE-SIZE(#300):\n".to_string(),
        "// FREE-SIZE(#300): ok\n".to_string(),
        "// FREE-SIZE: the preview needs a lot of room\n".to_string(),
        "// FREE-SIZE(#299): the preview needs a lot of room\n".to_string(),
        format!("// FREE-SIZE(#300): the preview needs a lot of room{far}"),
    ] {
        let code = format!(
            "fn f() {{\n{before}let _ = DialogNature::Draft(InteractiveSize::Free(vec2(1.0, 2.0)));\n}}"
        );
        assert_eq!(
            free_sizes_without_justification(&[file("app/w.rs", &code)]).len(),
            1,
            "exception wrongly accepted: {code}"
        );
    }
    for code in [
        "fn f() {\n// FREE-SIZE(#300): the preview needs a lot of room\nlet _ = InteractiveSize::Free(v);\n}",
        "fn f() {\n// FREE-SIZE(#300): the preview needs a lot of room\n// (rest)\nlet _ = InteractiveSize::Free(v);\n}",
        "fn f() { // InteractiveSize::Free(v)\n let _ = \"InteractiveSize::Free(v)\"; }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g() { let _ = InteractiveSize::Free(v); } }",
        "fn f() { let _ = PlainSize::Content(v); let _ = InteractiveSize::Class(c); }",
    ] {
        assert!(
            free_sizes_without_justification(&[file("app/w.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
}

// ---------------------------------------------------------------------------
// A scroll area has an end margin or a justified exception (#347)
// ---------------------------------------------------------------------------

/// The marker of a scroll area left raw:
/// `// RAW-SCROLL(#347): <why no end margin>`.
const SCROLL_MARKER: &str = "RAW-SCROLL(#347):";

/// The raw constructors of a vertical scroll area.
const RAW_SCROLL_CALLS: [&str; 2] = ["ScrollArea::vertical(", "ScrollArea::both("];

/// The raw vertical scroll areas of the production code outside `theme.rs`
/// without their justified exception, `path:line: reason`.
fn raw_scroll_areas_without_justification(sources: &[SourceFile]) -> Vec<String> {
    let mut found = Vec::new();
    for file in sources {
        if file.path == "theme.rs" {
            continue;
        }
        let source = file.production();
        for i in 0..source.mask.len() {
            if !RAW_SCROLL_CALLS
                .iter()
                .any(|call| starts_with_at(&source.mask, i, call))
            {
                continue;
            }
            let line = line_of(&source.original, i);
            let place = format!("src/{}:{line}", file.path);
            match marker_justification(&source.original, line, SCROLL_MARKER) {
                None => found.push(format!("{place}: no marker")),
                Some(text) if text.chars().count() < MINIMUM_JUSTIFICATION => {
                    found.push(format!("{place}: empty or too short justification"));
                }
                Some(_) => {}
            }
        }
    }
    found
}

/// Rule #347: a page that scrolls uses `theme::page_scroll_area()` (end
/// margin under the last control). A raw `ScrollArea::vertical()` /
/// `::both()` (tree, table, canvas, computed height) requires
/// `// RAW-SCROLL(#347): <justification>` in the twelve lines before.
#[test]
fn no_raw_scroll_area_without_justification() {
    let sources = sources::sources();
    assert!(
        sources.iter().any(|f| f.path == "theme.rs"),
        "theme.rs not read: the guard would no longer see the page_scroll_area helper"
    );
    let offenders = raw_scroll_areas_without_justification(&sources);
    assert!(
        offenders.is_empty(),
        "raw scroll area without justification — use `theme::page_scroll_area()` or write \
         `// {SCROLL_MARKER} <justification>` right before (docs/ui-principles.md): {offenders:?}"
    );
}

/// The witness: a bare area (vertical or both) is seen; an empty, too short,
/// wrong-number or too distant exception is seen too; a justified exception,
/// a comment, a string, test code and `theme.rs` pass.
#[test]
fn the_scroll_area_guard_sees_a_bare_area() {
    for bare in [
        "fn f() { egui::ScrollArea::vertical().show(ui, |ui| {}); }",
        "fn f() { egui::ScrollArea::both().show(ui, |ui| {}); }",
    ] {
        assert_eq!(
            raw_scroll_areas_without_justification(&[file("app/w.rs", bare)]).len(),
            1,
            "bare area missed: {bare}"
        );
    }
    let far = "\n".repeat(MARKER_SCOPE + 2);
    for before in [
        "// RAW-SCROLL(#347):\n".to_string(),
        "// RAW-SCROLL(#347): ok\n".to_string(),
        "// RAW-SCROLL: the tree owns its drop area\n".to_string(),
        "// RAW-SCROLL(#346): the tree owns its drop area\n".to_string(),
        format!("// RAW-SCROLL(#347): the tree owns its drop area{far}"),
    ] {
        let code =
            format!("fn f() {{\n{before}egui::ScrollArea::vertical().show(ui, |ui| {{}});\n}}");
        assert_eq!(
            raw_scroll_areas_without_justification(&[file("app/w.rs", &code)]).len(),
            1,
            "exception wrongly accepted: {code}"
        );
    }
    for code in [
        "fn f() {\n// RAW-SCROLL(#347): the tree owns its drop area\negui::ScrollArea::both().show(ui, |ui| {});\n}",
        "fn f() { // ScrollArea::vertical()\n let _ = \"ScrollArea::vertical()\"; }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g() { egui::ScrollArea::vertical(); } }",
        "fn f() { theme::page_scroll_area().show(ui, |ui| {}); egui::ScrollArea::horizontal(); }",
    ] {
        assert!(
            raw_scroll_areas_without_justification(&[file("app/w.rs", code)]).is_empty(),
            "false positive: {code}"
        );
    }
    let in_theme = "fn page_scroll_area() { egui::ScrollArea::vertical(); }";
    assert!(raw_scroll_areas_without_justification(&[file("theme.rs", in_theme)]).is_empty());
}

// ---------------------------------------------------------------------------
// The window catalog and the code agree (#300, S-F11)
// ---------------------------------------------------------------------------

/// The modal dialog functions: a `stokesforge_…` identifier passed to
/// one of them is that of a window of the catalog.
const MODAL_CALLS: [&str; 9] = [
    "modal_dialog_with",
    "modal_dialog",
    "modal_wizard",
    "modal_live_settings",
    "modal_read_only_with",
    "modal_read_only",
    "modal_confirm",
    "modal_save_changes",
    "modal_alert",
];

/// The identifiers that the code **builds** (a `&str` chosen before the call,
/// not a literal in its arguments): the guard does not see them in a call
/// and just looks for them as literals, then in the catalog.
const BUILT_IDENTIFIERS: [&str; 3] = [
    "stokesforge_plot_settings",
    "stokesforge_curve_settings",
    "stokesforge_analysis",
];

/// The `"stokesforge_…"` literals written in the arguments of a modal call
/// of `file`, `(identifier, line)`.
fn identifiers_passed_to_dialogs(file: &SourceFile) -> Vec<(String, usize)> {
    let source = file.production();
    let mut found = Vec::new();
    for i in 0..source.mask.len() {
        let Some(name) = MODAL_CALLS
            .iter()
            .find(|name| word_at(&source.mask, i, name))
        else {
            continue;
        };
        let after = i + name.len();
        let definition = i >= 3 && starts_with_at(&source.mask, i - 3, "fn ");
        if source.mask.get(after) != Some(&'(') || definition {
            continue;
        }
        let Some(end) = closing(&source.mask, after) else {
            continue;
        };
        // The identifier precedes the content closures: a `"stokesforge_…"`
        // of a grid or a scroll area placed there is not a window.
        let arguments: String = source.code[after..=end].iter().collect();
        let arguments = arguments.split('|').next().unwrap_or_default();
        for (k, _) in arguments.match_indices("\"stokesforge_") {
            let identifier: String = arguments[k + 1..]
                .chars()
                .take_while(|c| is_ident(*c))
                .collect();
            found.push((identifier, line_of(&source.original, i)));
        }
    }
    found
}

/// The `stokesforge_…` identifiers written between backticks in the
/// catalog.
fn catalog_identifiers(catalog: &str) -> Vec<String> {
    catalog
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| token.starts_with("stokesforge_") && token.chars().all(is_ident))
        .map(str::to_string)
        .collect()
}

/// The deviations between the modal calls of the code and the catalog: an
/// identifier passed to a dialog and absent from the catalog, a catalog line without a call.
fn catalog_deviations(sources: &[SourceFile], catalog: &str, built_ids: &[&str]) -> Vec<String> {
    let in_catalog = catalog_identifiers(catalog);
    let mut deviations = Vec::new();
    let mut called = Vec::new();
    let mut literals = String::new();
    for file in sources {
        literals.push_str(&file.production().code_as_text());
        for (identifier, line) in identifiers_passed_to_dialogs(file) {
            if !in_catalog.contains(&identifier) {
                deviations.push(format!(
                    "src/{}:{line}: `{identifier}` passed to a dialog, absent from docs/windows.md",
                    file.path
                ));
            }
            called.push(identifier);
        }
    }
    for identifier in &in_catalog {
        let built_id = built_ids.contains(&identifier.as_str())
            && literals.contains(&format!("\"{identifier}\""));
        if !called.contains(identifier) && !built_id {
            deviations.push(format!(
                "docs/windows.md: `{identifier}` without a dialog call in the code"
            ));
        }
    }
    for built_id in built_ids {
        if !in_catalog.iter().any(|id| id == built_id) {
            deviations.push(format!(
                "`{built_id}` (built identifier) absent from docs/windows.md"
            ));
        }
    }
    deviations
}

/// The window catalog.
fn window_catalog() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("windows.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} unreadable: {e}", path.display()))
}

/// Rule #300 (S-F11): `docs/windows.md` is the window catalog. Every
/// `stokesforge_…` identifier passed to a `modal_*` function appears in it, and
/// every catalog line has its call: a new window enters the catalog
/// in the same commit, a removed window leaves it. The identifiers built
/// before the call (`BUILT_IDENTIFIERS`) are checked as literals.
/// **Does not check** the sizes, the types nor the pages: they are left to
/// review.
#[test]
fn the_window_catalog_and_the_code_agree() {
    let sources = sources::sources();
    assert!(
        sources.iter().any(|f| f.path == "app/confirmations.rs"),
        "app/confirmations.rs not read: the guard would no longer see the blocking dialogs"
    );
    let deviations = catalog_deviations(&sources, &window_catalog(), &BUILT_IDENTIFIERS);
    assert!(deviations.is_empty(), "{deviations:?}");
}

/// The witness: an identifier passed to a dialog but absent from the catalog is
/// seen, under each modal function; a catalog line without a call is seen
/// too; a built identifier without a literal is seen too; a cataloged call,
/// a present built identifier, a definition, a comment
/// and test code pass.
#[test]
fn the_catalog_guard_sees_a_deviation() {
    let catalog = "| Window | `stokesforge_known` | `stokesforge_built` |";
    let built_id = ["stokesforge_built"];
    let base = "fn g() { let _ = \"stokesforge_built\"; }";
    let call = "fn h() { modal_dialog(egui::Id::new(\"stokesforge_known\")); }";
    let with_code = |code: &str| {
        vec![
            file("app/w.rs", code),
            file("app/c.rs", base),
            file("app/d.rs", call),
        ]
    };
    assert!(catalog_deviations(&with_code("fn f() {}"), catalog, &built_id).is_empty());
    for name in MODAL_CALLS {
        let code = format!("fn f() {{ {name}(ctx, egui::Id::new(\"stokesforge_unknown\"), t); }}");
        assert_eq!(
            catalog_deviations(&with_code(&code), catalog, &built_id).len(),
            1,
            "not seen: {name}"
        );
    }
    // A catalog line without a call.
    let alone = [file("app/c.rs", base)];
    assert_eq!(catalog_deviations(&alone, catalog, &built_id).len(), 1);
    // A built identifier without a literal in the code.
    let without_literal = [file("app/d.rs", call)];
    assert_eq!(
        catalog_deviations(&without_literal, catalog, &built_id).len(),
        1
    );
    for code in [
        "fn modal_dialog(ctx: &Context, id: Id) {}",
        "fn f() { // modal_alert(ctx, Id::new(\"stokesforge_unknown\"))\n }",
        "fn f() {}\n#[cfg(test)]\nmod tests { fn g() { modal_alert(Id::new(\"stokesforge_unknown\")); } }",
        "fn f() { modal_dialog_with(ctx, id, t, n); }",
    ] {
        assert!(
            catalog_deviations(&with_code(code), catalog, &built_id).is_empty(),
            "false positive: {code}"
        );
    }
}
