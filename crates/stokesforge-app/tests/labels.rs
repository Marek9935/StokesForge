//! Translation guards (issue #41, work package #39).
//!
//! # `no_hardcoded_label`
//!
//! No string literal in the calls that put a text on screen
//! (`label`, `button`, `menu_button`, `checkbox`, `radio_value`, `set_title`,
//! `selectable_value`, `selectable_label`, `on_hover_text`, `heading`…), nor in
//! the widgets built by `Button::new("…")` / `RichText::new("…")`, nor in the
//! `fn label()` / `fn title()` tables: every text goes through `tr!` with a key
//! of the `en` catalog. Model: `no_window_is_a_raw_egui_window`
//! (`tests/guards.rs`) — the test reads the sources and refuses. All the sources
//! of `src/` are read: a new file is guarded without declaring anything.
//!
//! A literal with no letter at all (`"→"`, `" · "`, `"{}"`) is not a text to
//! translate and passes; neither is the key of a `tr!("…")`.
//! For `add_filter` (#127), only the first argument is a label: the
//! extensions are technical, like the format acronyms (`SVG`, `PDF`, `PNG`).
//!
//! The sources are read by `support/sources.rs` (#116): all of `src/`,
//! subdirectories included, and the test code removed item by item — a
//! file that alternates code and test modules (`errors.rs`) stays guarded
//! after its first `#[cfg(test)]`.
//!
//! ## An absolute guard (#48)
//!
//! No exemption: a text literal in one of these calls makes the test fail,
//! whatever the file. (The list from before #41, 213 literals in 14 zones,
//! was reduced to zero zone by zone, #42 to #45, then removed.)
//!
//! ## What the guard does not see
//!
//! A text placed by another path: a `&str` built elsewhere then passed as a
//! variable, a title of `modal_dialog_with` / `window_header` / `tab_strip`
//! (their arguments mix text and identifiers), a computed column or unit name.
//! [`ZONE_43`] guards the functions of the graph properties window more
//! strictly; the pseudo-locale capture (`STOKESFORGE_LANG=en-XA`) shows what
//! escaped: a text without `[[ ]]`.

use std::path::Path;

#[path = "support/sources.rs"]
mod sources;

use sources::{
    body, closing, functions, functions_named, ident_at, is_ident, line_of, short_name,
    starts_with_at, Mask, SourceFile,
};

/// The `.name(…)` calls where no argument may be a text literal.
const CALLS: &[&str] = &[
    "label",
    "button",
    "small_button",
    "menu_button",
    "checkbox",
    "radio",
    "radio_value",
    "selectable_value",
    "selectable_label",
    "toggle_value",
    "on_hover_text",
    "on_disabled_hover_text",
    "heading",
    "hint_text",
    "selected_text",
    "with_title",
    "set_title",
    "add_filter",
    "strong",
    "weak",
    "small",
    "monospace",
    "colored_label",
    "on_hover_text_at_pointer",
    "notice",
    "warn",
    "fail",
];

/// The free functions of the application that place a label (#169):
/// `theme::section_title(ui, "…")`, the three text levels
/// (`section_heading`, `table_header`, `secondary_text`, #170), `theme::bold("…")`,
/// the list row.
/// Seen called alone or through a path (`crate::theme::bold(`), never
/// as a method.
const FREE_FUNCTIONS: &[&str] = &[
    "section_title",
    "section_heading",
    "table_header",
    "secondary_text",
    "bold",
    "list_row",
    "list_row_text",
];

/// The widgets built by `Type::new("text")`: `egui::Button::new("Open…")`
/// is the form of almost every menu entry, and `RichText::new(…)` that
/// of a styled text.
const CONSTRUCTORS: &[&str] = &[
    "Button",
    "Checkbox",
    "CollapsingHeader",
    "Label",
    "RichText",
];

/// The functions whose body is a table of labels: no text literal inside.
/// The last five are the tables of the import wizard
/// (#44, `data_end_label` moved out of `data_end_row` by #118);
/// `encoding_label` and `column_role_label` are not there: their technical
/// names (`UTF-8`, `Windows-1252`, `X`, `Y`) are legitimate literals.
const TABLES: &[&str] = &[
    "label",
    "title",
    "separator_label",
    "thousands_label",
    "decimal_label",
    "column_kind_label",
    "data_end_label",
];

// ---------------------------------------------------------------------------
// The violations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    file: String,
    /// `Type::name` in an `impl` block, `name` otherwise.
    function: String,
    line: usize,
    call: String,
    excerpt: String,
}

/// The opening positions of the **text** literals of `mask[start..end]`
/// — those that carry at least one letter — without those of a `tr!(…)`, whose
/// literal is the catalog key.
fn text_literals(mask: &[char], start: usize, end: usize) -> Vec<usize> {
    let mut found = Vec::new();
    let mut i = start;
    while i < end {
        if starts_with_at(mask, i, "tr!(") && (i == 0 || !is_ident(mask[i - 1])) {
            match closing(mask, i + 3) {
                Some(f) => {
                    i = f + 1;
                    continue;
                }
                None => break,
            }
        }
        if mask[i] == '"' {
            let mut j = i + 1;
            while j < end && mask[j] != '"' {
                j += 1;
            }
            if mask[i + 1..j.min(end)].contains(&'A') {
                found.push(i);
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    found
}

/// The literal as written, from its opening quote.
fn excerpt(source: &Mask, opening_quote: usize) -> String {
    let mut end = opening_quote + 1;
    while end < source.mask.len() && source.mask[end] != '"' {
        end += 1;
    }
    source.original[opening_quote..=end.min(source.original.len() - 1)]
        .iter()
        .collect()
}

/// The comma that ends a filter's name, without confusing those of a
/// `tr!`, `format!` or nested block with the argument separation.
/// In the mask, the commas of strings are already erased.
fn first_argument_end(mask: &[char], start: usize, end: usize) -> usize {
    let mut i = start;
    while i < end {
        match mask[i] {
            ',' => return i,
            '(' | '[' | '{' => i = closing(mask, i).unwrap_or(end),
            _ => {}
        }
        i += 1;
    }
    end
}

/// All the violations of a file, test code excluded.
fn violations(
    file: &SourceFile,
    calls: &[&str],
    constructeurs: &[&str],
    tables: &[&str],
) -> Vec<Violation> {
    let source = file.production();
    let mask = &source.mask;
    let path = format!("src/{}", file.path);
    let fns = functions(mask);
    let function_at = |position: usize| -> String {
        fns.iter()
            .rev()
            .find(|(p, _)| *p < position)
            .map_or_else(|| "?".to_string(), |(_, name)| name.clone())
    };

    // (opening parenthesis, position of the call, call)
    let mut found_calls: Vec<(usize, usize, String)> = Vec::new();
    for i in 0..mask.len() {
        if mask[i] == '.' {
            let name = ident_at(mask, i + 1);
            let after = i + 1 + name.chars().count();
            if calls.contains(&name.as_str()) && mask.get(after) == Some(&'(') {
                found_calls.push((after, i, format!(".{name}(")));
            }
        } else if mask[i].is_lowercase()
            && (i == 0 || !(is_ident(mask[i - 1]) || mask[i - 1] == '.'))
        {
            let name = ident_at(mask, i);
            let after = i + name.chars().count();
            if FREE_FUNCTIONS.contains(&name.as_str()) && mask.get(after) == Some(&'(') {
                found_calls.push((after, i, format!("{name}(")));
            }
        } else if mask[i].is_uppercase() && (i == 0 || !is_ident(mask[i - 1])) {
            let name = ident_at(mask, i);
            let after = i + name.chars().count();
            if constructeurs.contains(&name.as_str()) && starts_with_at(mask, after, "::new(") {
                found_calls.push((after + 5, i, format!("{name}::new(")));
            }
        }
    }

    let mut found = Vec::new();
    // A text in `label(RichText::new("…"))` is counted only once.
    let mut seen = std::collections::BTreeSet::new();
    for (paren, position, call) in found_calls {
        let Some(f) = closing(mask, paren) else {
            continue;
        };
        let filtre = call == ".add_filter(";
        let end = if filtre {
            first_argument_end(mask, paren + 1, f)
        } else {
            f
        };
        for opening_quote in text_literals(mask, paren + 1, end) {
            let excerpt = excerpt(&source, opening_quote);
            // Same distinction as in zone #43: a format acronym
            // has no translation. This does not hold for a dialog title.
            if filtre
                && excerpt[1..excerpt.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                continue;
            }
            if seen.insert(opening_quote) {
                found.push(Violation {
                    file: path.clone(),
                    function: function_at(position),
                    line: line_of(&source.original, opening_quote),
                    call: call.clone(),
                    excerpt,
                });
            }
        }
    }

    // The `fn label()` / `fn title()` tables: every text literal of the body.
    for (position, qualified) in &fns {
        let name = short_name(qualified);
        if !tables.contains(&name) {
            continue;
        }
        let Some((brace, f)) = body(mask, *position) else {
            continue;
        };
        for opening_quote in text_literals(mask, brace + 1, f) {
            if seen.insert(opening_quote) {
                found.push(Violation {
                    file: path.clone(),
                    function: qualified.clone(),
                    line: line_of(&source.original, opening_quote),
                    call: format!("fn {name}()"),
                    excerpt: excerpt(&source, opening_quote),
                });
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// The guard
// ---------------------------------------------------------------------------

/// The violations of `sources`, one line each; empty if everything is in order.
fn deviations(sources: &[SourceFile]) -> Vec<String> {
    let mut deviations = Vec::new();
    for file in sources {
        for infraction in violations(file, CALLS, CONSTRUCTORS, TABLES) {
            deviations.push(format!(
                "{}:{} in `{}`: {} {} — hardcoded text, go through `tr!` with a key \
                 of `i18n/en/stokesforge_app.ftl` (skill `stokesforge-ui-texts`)",
                infraction.file,
                infraction.line,
                infraction.function,
                infraction.call,
                infraction.excerpt
            ));
        }
    }
    deviations
}

/// No interface text hardcoded in `src/`: everything goes through `tr!`.
#[test]
fn no_hardcoded_label() {
    let deviations = deviations(&sources::sources());
    assert!(
        deviations.is_empty(),
        "{} deviation(s):\n  {}",
        deviations.len(),
        deviations.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Zone #43, guarded without reserve
// ---------------------------------------------------------------------------

/// The functions of the graph properties window, of the text editor
/// on the graph and of the common footer of modal dialogs (#43). Zero
/// exemptions, and more strictly than [`CALLS`]: **no** text literal in
/// their body, whatever the form of the call (`strong`, `weak`, `color_field`,
/// `text_field`, `set_title`, an argument of a free function…) — only the
/// literals that are identifiers pass (`[a-z0-9_./-]`: egui identifier salts,
/// addresses), the format acronyms (`SVG`) and the `tr!` keys.
/// To be merged into the general guard when [`CALLS`] can see these forms
/// (#48).
///
/// An entry names a function by its last segment (`modal_dialog`: free
/// function or `Type::modal_dialog` method), looked up in **all** the
/// sources; an entry found nowhere makes the guard fail (#116)
/// — a renamed function does not leave the zone silently.
const ZONE_43: &[&str] = &[
    "plot_settings_window",
    "plot_settings_contents",
    "graph_settings_page",
    "plot_area_settings_page",
    "axes_settings_page",
    "curve_settings_page",
    "series_list",
    "series_page",
    "membership_editor",
    "gradient_editor",
    "theme_editor",
    "formula_help",
    "formula_field",
    "axis_editor",
    "series_editor",
    "legend_editor",
    "plot_type_label",
    "line_style_label",
    "symbol_shape_label",
    "legend_position_label",
    "annotation_editor_ui",
    "graph_view_contents",
    "request_graph_export",
    "modal_dialog",
    "modal_dialog_with",
];

/// The text literals, identifiers excluded, in the body of the `zone` functions
/// of `source` (production view): (function, line, literal).
fn bare_literals(source: &Mask, zone: &[&str]) -> Vec<(String, usize, String)> {
    let mask = &source.mask;
    let mut found = Vec::new();
    for (position, name) in functions(mask) {
        if !zone.contains(&short_name(&name)) {
            continue;
        }
        let Some((brace, end)) = body(mask, position) else {
            continue;
        };
        for opening_quote in text_literals(mask, brace + 1, end) {
            let literal = excerpt(source, opening_quote);
            let content = &literal[1..literal.len().saturating_sub(1)];
            let identifier = content
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_./-".contains(c));
            // A format acronym (`SVG`, `PDF`) has no translation.
            let acronym = content
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
            if !identifier && !acronym {
                found.push((
                    name.clone(),
                    line_of(&source.original, opening_quote),
                    literal,
                ));
            }
        }
    }
    found
}

/// The entries of `zone` that no production function of `sources` carries.
fn missing<'z>(sources: &[SourceFile], zone: &[&'z str]) -> Vec<&'z str> {
    zone.iter()
        .copied()
        .filter(|name| functions_named(sources, name).is_empty())
        .collect()
}

/// The bare literals of the zone in `sources`, one line each.
fn zone_deviations(sources: &[SourceFile], zone: &[&str]) -> Vec<String> {
    let mut deviations = Vec::new();
    for file in sources {
        for (function, line, literal) in bare_literals(&file.production(), zone) {
            deviations.push(format!(
                "src/{}:{line} in `{function}`: {literal}",
                file.path
            ));
        }
    }
    deviations
}

fn bare(code: &str, zone: &[&str]) -> Vec<(String, usize, String)> {
    bare_literals(&file("t.rs", code).production(), zone)
}

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.to_string(),
        text: text.to_string(),
        is_test: false,
    }
}

/// The graph properties window, its tabs, the text editor and the
/// footer of modal dialogs go **entirely** through `tr!`: no
/// exemption (#43).
#[test]
fn graph_properties_and_export_have_no_literal() {
    let sources = sources::sources();
    let missing = missing(&sources, ZONE_43);
    assert!(
        missing.is_empty(),
        "function(s) of zone #43 not found in src/: {missing:?} — renamed? \
         update ZONE_43 rather than letting it leave the guard"
    );
    let deviations = zone_deviations(&sources, ZONE_43);
    assert!(
        deviations.is_empty(),
        "{} hardcoded text(s) in zone #43:\n  {}",
        deviations.len(),
        deviations.join("\n  ")
    );
}

/// The witness of the zone guard: a literal in a call form that
/// [`CALLS`] does not see (`strong`, an argument of a free function) is refused;
/// an identifier and a `tr!` key pass; so does a function outside the zone.
#[test]
fn the_zone_43_guard_sees_the_forms_that_calls_ignores() {
    let zone = &["page"];
    for code in [
        r#"fn page(ui: &mut Ui) { ui.strong("Title"); }"#,
        r#"fn page(ui: &mut Ui) { field(ui, "Color", &mut x); }"#,
        r#"fn page(ui: &mut Ui) { let _ = ui.hyperlink("link"); let _ = Some("Auto"); }"#,
        r#"fn page(ui: &mut Ui) { let _ = format!("Axis {}", n); }"#,
    ] {
        assert_eq!(bare(code, zone).len(), 1, "not seen: {code}");
    }
    for code in [
        r#"fn page(ui: &mut Ui) { ui.strong(tr!("plot-tab-axes")); }"#,
        r#"fn page(ui: &mut Ui) { ui.id_salt("plot_settings_series_list"); }"#,
        r#"fn page(ui: &mut Ui) { let _ = tr!("a", url = "typst.app/docs/reference/math"); }"#,
        r#"fn elsewhere(ui: &mut Ui) { ui.strong("Title"); }"#,
        r#"fn page(d: Dialog) { d.add_filter("SVG", &["svg"]); }"#,
        "fn page(ui: &mut Ui) { let _ = \"→\"; }",
    ] {
        assert!(bare(code, zone).is_empty(), "false positive: {code}");
    }
}

/// An extracted page must not lose the strict guard of its former body.
#[test]
fn zone_43_guards_the_pages_extracted_from_properties() {
    for page in [
        "graph_settings_page",
        "plot_area_settings_page",
        "axes_settings_page",
        "curve_settings_page",
    ] {
        let code =
            format!("fn {page}(ui: &mut Ui) {{ field(ui, \"Untranslated title\", &mut x); }}");
        assert_eq!(bare(&code, ZONE_43).len(), 1, "page left the guard: {page}");
    }
}

/// The witness of #116: a zone function moved out of `app.rs` —
/// into a module of `src/app/`, or become a method — stays guarded; its test
/// copy does not; a zone entry that is not found is reported.
#[test]
fn zone_43_follows_its_functions_outside_app_rs() {
    let sources = [
        file(
            "app/properties.rs",
            "pub(crate) fn page(ui: &mut Ui) {\n    ui.strong(\"Title\");\n}\n\
             #[cfg(test)]\nmod tests { fn page(ui: &mut Ui) { ui.strong(\"Test\"); } }",
        ),
        file(
            "app/methods.rs",
            "impl App {\n    fn export(&mut self) { field(\"Format\"); }\n}",
        ),
    ];
    let deviations = zone_deviations(&sources, &["page", "export"]);
    assert_eq!(
        deviations,
        [
            "src/app/properties.rs:2 in `page`: \"Title\"",
            "src/app/methods.rs:2 in `App::export`: \"Format\"",
        ]
    );
    assert_eq!(
        missing(&sources, &["page", "export", "vanished"]),
        ["vanished"]
    );
    // A function that exists only in tests is not found, not guarded.
    let test_only = [file("app/a.rs", "#[cfg(test)]\nmod tests { fn page() {} }")];
    assert_eq!(missing(&test_only, &["page"]), ["page"]);
}

// ---------------------------------------------------------------------------
// The witnesses: without them, a guard that sees nothing would pass for green
// ---------------------------------------------------------------------------

fn find_violations(code: &str) -> Vec<Violation> {
    violations(&file("t.rs", code), CALLS, CONSTRUCTORS, TABLES)
}

/// #169: a literal passed to a section title, a bold or a list row
/// is seen; a `tr!` key, a homonymous method and a definition
/// pass.
#[test]
fn the_guard_sees_the_labels_of_the_free_theme_functions() {
    for code in [
        r#"fn f(ui: &mut Ui) { section_title(ui, "Échelle"); }"#,
        r#"fn f(ui: &mut Ui) { crate::theme::section_title(ui, "Échelle"); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(theme::bold("Title")); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(theme::section_heading("Title")); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(theme::table_header("Value")); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(theme::secondary_text("Detail")); }"#,
        r#"fn f(ui: &mut Ui) { list_row(ui, None, "Series", true, false); }"#,
    ] {
        assert_eq!(find_violations(code).len(), 1, "not seen: {code}");
    }
    for code in [
        r#"fn f(ui: &mut Ui) { section_title(ui, tr!("plot-axes-scale-section")); }"#,
        r#"fn f(x: X) { x.bold("Title"); }"#,
        "pub fn section_title(ui: &mut Ui, text: impl Into<String>) -> Response { todo!() }",
        r#"fn f() { my_bold("Title"); }"#,
    ] {
        assert!(find_violations(code).is_empty(), "false positive: {code}");
    }
}

/// #127: the titles and names of native filters are interface texts,
/// even in a submodule; the extensions are technical data.
#[test]
fn the_guard_sees_file_dialog_titles_and_filter_names() {
    for (code, expected) in [
        (
            r#"fn open_file() { dialog.set_title("Open a project"); }"#,
            1,
        ),
        (
            r#"fn open_file() { dialog.add_filter("StokesForge project", &["stf"]); }"#,
            1,
        ),
        (
            r#"fn open_file() { dialog.add_filter("SVG images, PDF documents", &["svg", "pdf"]); }"#,
            1,
        ),
        (
            r#"fn open_file() { dialog.add_filter(format!("Project {}", name), &["stf"]); }"#,
            1,
        ),
        (
            r#"fn open_file() { dialog.add_filter(format!("{} {}", tr!("filter"), "projects"), &["stf"]); }"#,
            1,
        ),
        (
            r#"fn open_file() { dialog.add_filter(if a { "Projects" } else { "All files" }, &["stf"]); }"#,
            2,
        ),
    ] {
        let report = deviations(&[file("app/files.rs", code)]);
        assert_eq!(report.len(), expected, "not seen: {code}\n{report:?}");
        assert!(report.iter().all(|e| e.contains("src/app/files.rs:1")));
    }
}

#[test]
fn the_filters_guard_accepts_translations_and_technical_formats() {
    for code in [
        r#"fn open_file() { dialog.set_title(tr!("project-open-title")); }"#,
        r#"fn open_file() { dialog.add_filter(tr!("project-file-filter"), &["stf"]); }"#,
        r#"fn open_file() { dialog.add_filter(tr!("filter", count = f(a, b)), &["stf"]); }"#,
        r#"fn export_file() { dialog.add_filter("SVG", &["svg"]).add_filter("PDF", &["pdf"]).add_filter("PNG", &["png"]); }"#,
    ] {
        assert!(find_violations(code).is_empty(), "false positive: {code}");
    }
    // A title is not a format name: even a text in capitals is guarded.
    assert_eq!(
        find_violations(r#"fn open_file() { dialog.set_title("OPEN"); }"#).len(),
        1
    );
}

#[test]
fn the_guard_sees_a_literal_in_every_call_form() {
    for code in [
        r#"fn f(ui: &mut Ui) { ui.label("Open"); }"#,
        r#"fn f(ui: &mut Ui) { ui.menu_button("File", |ui| {}); }"#,
        r#"fn f(ui: &mut Ui) { ui.checkbox(&mut b, "Show"); }"#,
        r#"fn f(ui: &mut Ui) { ui.add(egui::Button::new("Import…")); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(egui::RichText::new("Title").strong()); }"#,
        r#"fn f(ui: &mut Ui) { ui.button(x).on_hover_text("Help"); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(format!("Axis {}", n)); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(format!("{} {}", tr!("a"), "b")); }"#,
        r##"fn f(ui: &mut Ui) { ui.label(r#"raw"#); }"##,
        r#"fn f() { let _ = ViewportBuilder::default().with_title("StokesForge"); }"#,
        // The status bar (#216): direct literal, through `format!`, or on a new line.
        r#"fn f(&mut self) { self.notice("Terminé".to_string()); }"#,
        r#"fn f(&mut self) { self.fail(format!("Failed: {}", e)); }"#,
        "fn f(&mut self) {\n    self.fail(\n        \"Impossible\".to_string(),\n    );\n}",
    ] {
        assert_eq!(find_violations(code).len(), 1, "not seen: {code}");
    }
}

#[test]
fn a_label_with_two_branches_counts_each_literal() {
    let code = r#"fn f(ui: &mut Ui) { ui.label(if a { "Yes" } else { "No" }); }"#;
    assert_eq!(find_violations(code).len(), 2);
}

#[test]
fn styled_text_in_a_label_is_counted_only_once() {
    let code = r#"fn f(ui: &mut Ui) { ui.label(RichText::new("Title")); }"#;
    assert_eq!(find_violations(code).len(), 1);
}

#[test]
fn the_guard_lets_through_what_is_not_a_label() {
    for code in [
        r#"fn f(ui: &mut Ui) { ui.label(tr!("menu-file")); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(tr!("witness-count", count = 2)); }"#,
        r#"fn f(ui: &mut Ui) { ui.add(egui::Button::new(tr!("menu-file"))); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(name); }"#,
        r#"fn f(&mut self) { self.fail(tr!("error-x", cause = e)); self.notice(text); }"#,
        r#"fn f(ui: &mut Ui) { ui.label("→"); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(" · "); }"#,
        r#"fn f(ui: &mut Ui) { ui.label(format!("{}", n)); }"#,
        r#"fn f(ui: &mut Ui) { ui.colored_label(c, format!("{code} {label}")); }"#,
        r#"fn f(ui: &mut Ui) { ui.separator(); ui.id_salt("graph"); }"#,
        "fn f(ui: &mut Ui) {\n    // ui.label(\"comment\");\n}",
        "fn f(ui: &mut Ui) { let _ = \"ui.label(\\\"in a string\\\")\"; }",
        r#"fn f(ui: &mut Ui) { let c = '"'; ui.label(tr!("a")); }"#,
        "fn f(ui: &mut Ui) { ui.label(tr!(\"a\")); }\n#[cfg(test)]\nmod t { fn g(ui: &mut Ui) { ui.label(\"test\"); } }",
    ] {
        assert!(find_violations(code).is_empty(), "false positive: {code}");
    }
}

#[test]
fn the_guard_reads_the_label_tables() {
    let code = "impl Tab {\n    pub fn label(self) -> &'static str {\n        match self {\n            Tab::A => \"Axes\",\n            Tab::B => \"Series\",\n        }\n    }\n}\n";
    let found = find_violations(code);
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|i| i.function == "Tab::label"));
    let converted = "impl Tab {\n    pub fn label(self) -> String {\n        match self {\n            Tab::A => tr!(\"tab-axes\"),\n        }\n    }\n}\n";
    assert!(find_violations(converted).is_empty());
    // A `;` of an array in the signature does not hide the body (#116).
    let array = "impl Tab {\n    fn label(self, _c: [u8; 4]) -> &'static str {\n        \"Axes\"\n    }\n}\n";
    assert_eq!(find_violations(array).len(), 1);
    // Another function does not read its literals as a table.
    assert!(find_violations("fn name() -> &'static str { \"Axes\" }").is_empty());
}

#[test]
fn the_guard_names_the_function_and_the_impl_block() {
    let code = "impl<T: Copy> Window<T> {\n    fn menu(&self, ui: &mut Ui) { ui.label(\"x y\"); }\n}\nfn free(ui: &mut Ui) { ui.label(\"x y\"); }\nimpl Drawing for Window {\n    fn paints(&self, ui: &mut Ui) { ui.label(\"x y\"); }\n}\n";
    let names: Vec<String> = find_violations(code)
        .into_iter()
        .map(|i| i.function)
        .collect();
    assert_eq!(names, ["Window::menu", "free", "Window::paints"]);
}

#[test]
fn the_guard_reports_each_literal_with_its_file_and_function() {
    let sources = vec![file(
        "t.rs",
        "fn menu(ui: &mut Ui) { ui.label(\"one\"); ui.label(\"two\"); }\nfn other(ui: &mut Ui) { ui.label(tr!(\"ok\")); }\n",
    )];
    let report = deviations(&sources);
    assert_eq!(report.len(), 2, "{report:?}");
    assert!(report
        .iter()
        .all(|e| e.contains("src/t.rs:1") && e.contains("`menu`")));
}

/// The witness of #116: a hardcoded label in a subdirectory of `src/`, or
/// placed after a test module (`errors.rs` alternates both), is seen; the
/// same in a test file is not.
#[test]
fn the_guard_sees_subdirectories_and_code_after_a_test_module() {
    let code = "mod a {\n    #[cfg(test)]\n    mod tests { fn t(ui: &mut Ui) { ui.label(\"test\"); } }\n}\n\
                fn menu(ui: &mut Ui) { ui.label(\"Ouvrir\"); }\n";
    let report = deviations(&[file("app/grid/menu.rs", code)]);
    assert_eq!(report.len(), 1, "{report:?}");
    assert!(
        report[0].starts_with("src/app/grid/menu.rs:5 in `menu`"),
        "{report:?}"
    );
    let mut test_files = file("app/grid/tests.rs", code);
    test_files.is_test = true;
    assert!(deviations(&[test_files]).is_empty());
}

/// The translation dependencies exist only in the application, that is
/// `stokesforge-app` and `stokesforge-app-state` (which carries the catalog, `tr!` and
/// the loader, #263): no crate below translates (work package #39). The
/// allowlists of `no_ui_dependency` already cover part of it; this one
/// covers the rest of the workspace.
#[test]
fn translation_dependencies_stay_in_stokesforge_app() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let forbidden = [
        "i18n-embed",
        "i18n-embed-fl",
        "fluent",
        "fluent-syntax",
        "rust-embed",
    ];
    let mut seen = 0;
    for entry in std::fs::read_dir(crates)
        .expect("crates directory")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let manifest = entry.path().join("Cargo.toml");
        let Ok(content) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        seen += 1;
        if name == "stokesforge-app" || name == "stokesforge-app-state" {
            continue;
        }
        for line in content.lines() {
            let key = line.split('=').next().unwrap_or("").trim();
            assert!(
                !forbidden.contains(&key),
                "{name} depends on `{key}`: only the application translates (#39)"
            );
        }
    }
    assert!(seen > 10, "the workspace was not walked");
}

// ---------------------------------------------------------------------------
// The catalog and its keys (#48, #46): read with the sources of BOTH crates
// ---------------------------------------------------------------------------

/// The `en` catalog, which lives in `stokesforge-app-state` (#263); its keys
/// are used in the sources of both crates.
const EN: &str = include_str!("../../stokesforge-app-state/i18n/en/stokesforge_app.ftl");

/// The keys of the messages of `catalog` (one per line that starts an entry;
/// attributes and `-x` terms are not).
fn catalog_keys(catalog: &str) -> Vec<&str> {
    catalog
        .lines()
        .filter(|line| line.starts_with(|c: char| c.is_ascii_lowercase()))
        .filter_map(|line| line.split(" =").next())
        .filter(|key| !key.contains(' ') && !key.contains('.'))
        .collect()
}

/// The keys of `catalog` that the text of no source (tests included: a key
/// used by a test counts) cites between quotes.
fn dead_keys<'c>(catalog: &'c str, sources: &[SourceFile]) -> Vec<&'c str> {
    let text: String = sources.iter().map(|f| f.text.as_str()).collect();
    catalog_keys(catalog)
        .into_iter()
        .filter(|key| !text.contains(&format!("\"{key}\"")))
        .collect()
}

/// A key of the `en` catalog that the code uses nowhere is a dead key
/// (a text believed visible, or the remains of a conversion): it makes this
/// test fail (#48). A key is used when its name, between quotes,
/// appears in a source of `src/` — the recursive list, of both crates,
/// shared by all the guards that read the code (#116, #263).
#[test]
fn no_catalog_key_is_dead() {
    let keys = catalog_keys(EN);
    assert!(keys.len() > 500, "the catalog is read: {} keys", keys.len());
    let dead = dead_keys(EN, &sources::sources());
    assert!(
        dead.is_empty(),
        "{} key(s) of `en` unused in `src/`: {dead:?}",
        dead.len()
    );
}

/// The witness: the menus and the rest of the interface are written in
/// `stokesforge-app`, the catalog lives in `stokesforge-app-state`. Read on
/// the state crate alone, the list of dead keys is not empty (the `menu-*`):
/// the guard holds only because it reads both.
#[test]
fn the_dead_keys_guard_reads_both_crates() {
    let state: Vec<SourceFile> = sources::sources()
        .into_iter()
        .filter(|f| f.path.starts_with(sources::STATE_PREFIX))
        .collect();
    assert!(!state.is_empty());
    let dead = dead_keys(EN, &state);
    assert!(
        dead.iter().any(|key| key.starts_with("menu-")),
        "the menus are used by stokesforge-app only: {dead:?}"
    );
    // And a key cited only in a comment stays used (the raw text
    // is read): the witness of the reading itself.
    let file = SourceFile {
        path: "app/x.rs".into(),
        text: "fn f() { tr!(\"key-a\"); }".into(),
        is_test: false,
    };
    assert_eq!(dead_keys("key-a = A\nkey-b = B\n", &[file]), ["key-b"]);
}

/// The state's `notice`/`fail` is guarded: the status bar messages
/// (`AppState::notice`/`fail`) are `tr!` calls, and the guard reads `state.rs` — from
/// the neighbor crate since #263 (prefix `app-state/`). Removing that crate from
/// the reading makes this test fail: without it, most messages would no
/// longer be guarded.
#[test]
fn the_guard_reads_the_state_messages_of_the_state_crate() {
    let sources = sources::sources();
    let state = sources
        .iter()
        .find(|f| f.path == "app-state/state.rs")
        .expect("state.rs is read");
    let calls = state
        .production()
        .code_as_text()
        .matches(".notice(")
        .count();
    assert!(
        calls > 10,
        "the `notice` calls of state.rs are read: {calls}"
    );
    // The witness: a literal placed there would be seen.
    let mut offender = state.clone();
    offender
        .text
        .push_str("\nimpl AppState { fn t(&mut self) { self.notice(\"Terminé\".into()); } }\n");
    assert_eq!(deviations(&[offender]).len(), 1);
    assert!(deviations(std::slice::from_ref(state)).is_empty());
}

/// The `history-*` keys of `catalog` that no production source uses
/// (key name between quotes).
fn history_keys_unused(catalog: &str, sources: &[sources::SourceFile]) -> Vec<String> {
    let code: String = sources
        .iter()
        .map(|file| file.production().code_as_text())
        .collect();
    history_keys(catalog)
        .into_iter()
        .filter(|key| !code.contains(&format!("\"{key}\"")))
        .map(str::to_string)
        .collect()
}

fn history_keys(catalog: &str) -> Vec<&str> {
    catalog
        .lines()
        .filter_map(|line| line.split(" =").next())
        .filter(|key| key.starts_with("history-") && !key.contains(' '))
        .collect()
}

/// Every `history-*` key of the catalog is used: an orphan key (a
/// form removed from the Rust, left in the `.ftl`) makes the test fail. All
/// the production sources of both `src/` are read, subdirectories included
/// (#116, #263): the sentence-making is in `stokesforge-app-state`, but
/// `app/menus.rs` uses it.
#[test]
fn every_history_key_of_the_catalogue_is_used() {
    let catalog = EN;
    let keys = history_keys(catalog);
    assert!(keys.len() > 30, "the history catalog is read: {keys:?}");
    let unused = history_keys_unused(catalog, &sources::sources());
    assert!(unused.is_empty(), "key(s) used nowhere: {unused:?}");
}

/// The witness: a key used in a module of `src/app/` counts; a key
/// used only in a comment or in test code does not.
#[test]
fn a_history_key_is_seen_outside_the_three_old_files() {
    let catalog = "history-a = A\nhistory-b = B\nhistory-c = C\nother = O\n";
    let file = |path: &str, text: &str| sources::SourceFile {
        path: path.to_string(),
        text: text.to_string(),
        is_test: false,
    };
    let sources = [
        file("app/history.rs", "fn f() { tr!(\"history-a\"); }"),
        file(
            "app/b.rs",
            "// tr!(\"history-b\")\n#[cfg(test)]\nmod tests { fn t() { tr!(\"history-b\"); } }",
        ),
    ];
    assert_eq!(
        history_keys_unused(catalog, &sources),
        ["history-b", "history-c"]
    );
}
