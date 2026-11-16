//! `pyst.apply_theme` and `pyst.themes` (#546): a graph theme applied by name
//! from a script, in the script's undoable group.
#![cfg(feature = "python")]

use std::path::PathBuf;

use stokesforge_commands::{HistoryLabel, MacroKind};
use stokesforge_core::{Column, NodeId, NodeKind, Project};
use stokesforge_plot::{builtin_themes, GraphTheme, PlotSpec, ThemeFamilies, ThemeFamily};
use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::{ScriptEngine, ThemeLibrary, ThemeLibraryEntry, ThemeSource};
use stokesforge_session::{Session, SessionHandle};

struct Bench {
    engine: ScriptEngine,
    session: SessionHandle,
    _actor: Session,
}

/// A project with `[Book1]Sheet1` (X, Y) and the given project themes.
fn bench_with(project_themes: Vec<GraphTheme>) -> Bench {
    let mut project = Project::new("Themes").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1., 2., 3.]))
        .unwrap();
    project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[2., 4., 8.]))
        .unwrap();
    project.set_project_themes(project_themes).unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let session = actor.handle();
    let home = std::env::var_os(interpreter::HOME_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        });
    let engine = ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(home)).unwrap();
    Bench {
        engine,
        session,
        _actor: actor,
    }
}

fn bench() -> Bench {
    bench_with(Vec::new())
}

const HANDLES: &str = "x = pyst.resolve('[Book1]Sheet1!X')\ny = pyst.resolve('[Book1]Sheet1!Y')\n";

fn run(b: &Bench, source: &str) -> String {
    let out = b.engine.run(source).unwrap();
    assert!(out.error.is_none(), "{:?}", out.error);
    out.stdout
}

/// The script must raise `kind`; returns the message. `g` is the graph of
/// [`plot_graph`] (the namespace lives on from one run to the next).
fn refused(b: &Bench, kind: &str, source: &str) -> String {
    let body = source
        .lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    let script = format!(
        "try:\n{body}\n    print('NOERROR')\nexcept pyst.{kind} as e:\n    print('REFUSED|' + str(e))\n"
    );
    let output = run(b, &script);
    output
        .lines()
        .find_map(|l| l.strip_prefix("REFUSED|"))
        .unwrap_or_else(|| panic!("expected {kind}, got {output:?}"))
        .to_owned()
}

/// Undoes the last run's group, between two applications.
fn undo(b: &Bench) {
    b.session.undo().unwrap();
}

fn graph_spec(b: &Bench) -> PlotSpec {
    let snap = b.session.snapshot().unwrap();
    let id = snap.tree.resolve("[Graph1]").unwrap();
    match snap.tree.node(id).unwrap().kind() {
        NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("graph expected"),
    }
}

fn builtin(key: &str) -> GraphTheme {
    builtin_themes::original(builtin_themes::index_of(key).unwrap()).unwrap()
}

/// The spec `themed_spec` gives `before` with `key`: what the application
/// would apply.
fn expected(
    b: &Bench,
    before: &PlotSpec,
    theme: &GraphTheme,
    families: ThemeFamilies,
    add: bool,
) -> PlotSpec {
    let snap = b.session.snapshot().unwrap();
    stokesforge_render_svg::themed_spec(
        &snap.tree,
        before,
        theme,
        families,
        add,
        &stokesforge_render_svg::FigureTexts::default(),
    )
}

fn default_families() -> ThemeFamilies {
    stokesforge_plot::default_theme_families()
}

/// A library like the application's: every built-in under its key, English
/// name and `french` names, then `users` (named by their own name).
fn library(french: &[(&str, &str)], users: Vec<GraphTheme>) -> ThemeLibrary {
    let mut entries = Vec::new();
    for (index, key) in builtin_themes::KEYS.iter().enumerate() {
        let theme = builtin_themes::original(index).unwrap();
        let mut names = vec![(*key).to_owned(), theme.name.clone()];
        names.extend(
            french
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, name)| (*name).to_owned()),
        );
        entries.push(ThemeLibraryEntry::new(
            ThemeSource::Builtin,
            format!("builtin:{key}"),
            theme.name.clone(),
            names,
            theme,
            false,
        ));
    }
    for (id, theme) in users.into_iter().enumerate() {
        entries.push(ThemeLibraryEntry::new(
            ThemeSource::User,
            format!("user:{}", id + 1),
            theme.name.clone(),
            vec![theme.name.clone()],
            theme,
            false,
        ));
    }
    ThemeLibrary::new(entries)
}

/// A user or project theme named `name`: the style of a graph with a thicker
/// curve, so that it differs from every built-in.
fn named(name: &str, width: f64) -> GraphTheme {
    let mut spec = PlotSpec::empty("Graph1");
    spec.panels.get_mut(0).unwrap().series.push({
        let mut series =
            stokesforge_plot::SeriesSpec::new_xy(NodeId::from_raw(1), NodeId::from_raw(2));
        series.line.width = width;
        series
    });
    GraphTheme::capture(name, &spec)
}

fn plot_graph(b: &Bench) -> PlotSpec {
    run(b, &format!("{HANDLES}g = pyst.plot(y, x, name='Graph1')"));
    graph_spec(b)
}

// --- Resolution by name --------------------------------------------------------------

#[test]
fn a_built_in_is_applied_by_its_key() {
    let b = bench();
    let before = plot_graph(&b);
    run(&b, "pyst.apply_theme(g, 'publication-wide')");
    let want = expected(
        &b,
        &before,
        &builtin("publication-wide"),
        default_families(),
        false,
    );
    assert_eq!(graph_spec(&b), want);
    assert_ne!(want, before, "the theme changed something");
}

#[test]
fn a_built_in_is_found_by_its_name_whatever_the_case_and_spaces() {
    let b = bench();
    b.engine.set_theme_library(Some(library(
        &[("publication-wide", "Publication large")],
        vec![],
    )));
    let before = plot_graph(&b);
    let want = expected(
        &b,
        &before,
        &builtin("publication-wide"),
        default_families(),
        false,
    );
    run(&b, "pyst.apply_theme(g, 'publication-wide')");
    for name in [
        "Publication Wide",
        " publication WIDE ",
        "Publication   Wide",
        "Publication large",
    ] {
        undo(&b);
        run(&b, &format!("pyst.apply_theme(g, {name:?})"));
        assert_eq!(graph_spec(&b), want, "{name}");
    }
}

#[test]
fn a_name_in_another_language_is_found_only_when_the_host_injected_it() {
    let b = bench();
    plot_graph(&b);
    let message = refused(
        &b,
        "UnknownThemeError",
        "pyst.apply_theme(g, 'Publication large')",
    );
    assert!(message.contains("Publication large"), "{message}");
}

#[test]
fn a_project_theme_is_found_without_any_injection() {
    let b = bench_with(vec![named("Fig A", 5.0)]);
    let before = plot_graph(&b);
    run(&b, "pyst.apply_theme(g, 'fig a')");
    let want = expected(&b, &before, &named("Fig A", 5.0), default_families(), false);
    assert_eq!(graph_spec(&b), want);
    assert_ne!(want, before);
}

#[test]
fn an_injected_user_theme_is_applied_and_unknown_headless_with_a_clear_message() {
    let b = bench();
    let before = plot_graph(&b);
    let poster = named("Poster", 6.0);
    // Headless: a user theme does not exist, and the message says why.
    let message = refused(&b, "UnknownThemeError", "pyst.apply_theme(g, 'Poster')");
    assert!(message.contains("application"), "{message}");
    b.engine
        .set_theme_library(Some(library(&[], vec![poster.clone()])));
    run(&b, "pyst.apply_theme(g, ' poster ')");
    assert_eq!(
        graph_spec(&b),
        expected(&b, &before, &poster, default_families(), false)
    );
}

#[test]
fn a_missing_name_raises_and_changes_neither_graph_nor_history() {
    let b = bench();
    plot_graph(&b);
    let before = b.session.snapshot().unwrap();
    let message = refused(
        &b,
        "UnknownThemeError",
        "pyst.apply_theme(g, 'No such theme')",
    );
    assert!(message.contains("No such theme"), "{message}");
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    // A kind of InvalidValueError, as documented.
    let kind = run(
        &b,
        "print(issubclass(pyst.UnknownThemeError, pyst.InvalidValueError))",
    );
    assert_eq!(kind.trim(), "True");
}

#[test]
fn a_prefix_restricts_the_source() {
    let b = bench_with(vec![named("Publication", 5.0)]);
    b.engine
        .set_theme_library(Some(library(&[], vec![named("Poster", 6.0)])));
    let before = plot_graph(&b);
    // `builtin:` skips the project theme that shares the name of a built-in.
    run(&b, "pyst.apply_theme(g, 'builtin:Publication')");
    assert_eq!(
        graph_spec(&b),
        expected(
            &b,
            &before,
            &builtin("publication"),
            default_families(),
            false
        )
    );
    undo(&b);
    run(&b, "pyst.apply_theme(g, 'project:publication')");
    assert_eq!(
        graph_spec(&b),
        expected(
            &b,
            &before,
            &named("Publication", 5.0),
            default_families(),
            false
        )
    );
    undo(&b);
    run(&b, "pyst.apply_theme(g, 'user:Poster')");
    assert_eq!(
        graph_spec(&b),
        expected(
            &b,
            &before,
            &named("Poster", 6.0),
            default_families(),
            false
        )
    );
    // The wrong source does not find it.
    refused(
        &b,
        "UnknownThemeError",
        "pyst.apply_theme(g, 'project:Poster')",
    );
    // The key `themes()` returns is accepted.
    undo(&b);
    run(&b, "pyst.apply_theme(g, 'builtin:kinetics')");
    assert_eq!(
        graph_spec(&b),
        expected(&b, &before, &builtin("kinetics"), default_families(), false)
    );
}

// --- Families and layers -------------------------------------------------------------

#[test]
fn the_default_families_keep_the_titles_and_the_axis_ranges() {
    let b = bench();
    plot_graph(&b);
    run(
        &b,
        "pyst.set_graph(g, title='Mine', x_title='Time', x_from=0.5, x_to=3.5)",
    );
    let before = graph_spec(&b);
    run(&b, "pyst.apply_theme(g, 'publication')");
    let after = graph_spec(&b);
    assert_eq!(after.title, "Mine");
    assert_eq!(after.panels[0].bottom.label, "Time");
    assert_eq!(after.panels[0].bottom.range_min, Some(0.5));
    assert_eq!(after.panels[0].bottom.range_max, Some(3.5));
    assert_ne!(after, before, "and the theme did apply");
}

#[test]
fn naming_the_families_applies_only_those() {
    let b = bench();
    let before = plot_graph(&b);
    run(
        &b,
        "pyst.apply_theme(g, 'publication', families=['curves'])",
    );
    let want = expected(
        &b,
        &before,
        &builtin("publication"),
        ThemeFamilies::only(ThemeFamily::Curves),
        false,
    );
    let got = graph_spec(&b);
    assert_eq!(got, want);
    assert_ne!(got, before);
    // Only the curves changed: the page and the axes are as they were.
    assert_eq!(got.panels[0].bottom, before.panels[0].bottom);
    assert_eq!(got.frame_widths, before.frame_widths);
    // Another way to write the same family: the underscore and a name.
    undo(&b);
    run(
        &b,
        "pyst.apply_theme(g, 'publication', families=['Size and page', 'axes'])",
    );
    let got = graph_spec(&b);
    assert_eq!(got.panels[0].series, before.panels[0].series);
    undo(&b);
    run(
        &b,
        "pyst.apply_theme(g, 'publication', families=('title_fonts',))",
    );
}

#[test]
fn bad_families_are_refused_and_nothing_is_written() {
    let b = bench();
    plot_graph(&b);
    let before = b.session.snapshot().unwrap();
    let message = refused(
        &b,
        "InvalidValueError",
        "pyst.apply_theme(g, 'publication', families=[])",
    );
    assert!(message.contains("empty"), "{message}");
    let message = refused(
        &b,
        "InvalidValueError",
        "pyst.apply_theme(g, 'publication', families=['colors'])",
    );
    assert!(
        message.contains("colors") && message.contains("title-fonts"),
        "{message}"
    );
    refused(
        &b,
        "InvalidValueError",
        "pyst.apply_theme(g, 'publication', families='curves')",
    );
    refused(
        &b,
        "InvalidValueError",
        "pyst.apply_theme(g, 'publication', families=[3])",
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
}

#[test]
fn missing_layers_are_added_only_on_request() {
    let b = bench();
    plot_graph(&b);
    run(&b, "pyst.apply_theme(g, 'fit-residuals')");
    assert_eq!(graph_spec(&b).panels.len(), 1);
    run(
        &b,
        "pyst.apply_theme(g, 'fit-residuals', add_missing_layers=True)",
    );
    assert_eq!(graph_spec(&b).panels.len(), 2);
}

// --- History -------------------------------------------------------------------------

#[test]
fn plot_then_apply_is_one_history_entry_and_one_undo_removes_all() {
    let b = bench();
    let start = b.session.snapshot().unwrap();
    run(
        &b,
        &format!("{HANDLES}g = pyst.plot(y, x, name='Graph1')\npyst.apply_theme(g, 'publication')"),
    );
    let after = b.session.snapshot().unwrap();
    assert_eq!(
        after.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Script))
    );
    b.session.undo().unwrap();
    let undone = b.session.snapshot().unwrap();
    assert_eq!(*undone.tree, *start.tree, "plot and theme gone together");
    assert!(!undone.history.can_undo);
}

#[test]
fn the_same_theme_twice_writes_nothing_the_second_time() {
    let b = bench();
    plot_graph(&b);
    run(&b, "pyst.apply_theme(g, 'publication')");
    let first = b.session.snapshot().unwrap();
    // A run whose first write is the theme opens the script's group itself.
    assert_eq!(
        first.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Script))
    );
    run(&b, "pyst.apply_theme(g, 'publication')");
    let second = b.session.snapshot().unwrap();
    assert_eq!(second.revision, first.revision);
    assert_eq!(second.history, first.history);
}

// --- Refusals ------------------------------------------------------------------------

#[test]
fn a_stale_handle_a_non_graph_and_hostile_names_are_refused() {
    let b = bench();
    plot_graph(&b);
    let before = b.session.snapshot().unwrap();
    let message = refused(
        &b,
        "InvalidValueError",
        "pyst.apply_theme(x, 'publication')",
    );
    assert!(message.contains("graph"), "{message}");
    refused(&b, "InvalidValueError", "pyst.apply_theme(g, 'a' * 10000)");
    refused(
        &b,
        "InvalidValueError",
        "pyst.apply_theme(g, 'pub\\x00lication')",
    );
    refused(&b, "UnknownThemeError", "pyst.apply_theme(g, '')");
    refused(&b, "UnknownThemeError", "pyst.apply_theme(g, 'builtin:')");
    refused(
        &b,
        "BrokenReferenceError",
        "pyst.remove(g)\npyst.apply_theme(g, 'publication')",
    );
    let _ = before;
}

// --- The list ------------------------------------------------------------------------

#[test]
fn themes_lists_built_ins_then_user_then_project_with_their_keys() {
    let b = bench_with(vec![named("Fig A", 5.0)]);
    let headless = run(
        &b,
        "print([(t['key'], t['source'], t['modified']) for t in pyst.themes()])\nprint(sorted(pyst.themes()[0].keys()))",
    );
    let lines: Vec<&str> = headless.lines().collect();
    let mut keys: Vec<String> = builtin_themes::KEYS
        .iter()
        .map(|k| format!("('builtin:{k}', 'builtin', False)"))
        .collect();
    keys.push("('project:fig a', 'project', False)".to_owned());
    assert_eq!(
        lines[0],
        format!("[{}]", keys.join(", ")),
        "headless: no user theme"
    );
    assert_eq!(lines[1], "['key', 'modified', 'name', 'source']");
    // With the application's library: the user's theme sits between.
    b.engine
        .set_theme_library(Some(library(&[], vec![named("Poster", 6.0)])));
    let injected = run(
        &b,
        "print([t['source'] for t in pyst.themes()])\nprint([t['name'] for t in pyst.themes()][-2:])",
    );
    let lines: Vec<&str> = injected.lines().collect();
    assert_eq!(
        lines[0],
        format!(
            "[{}'user', 'project']",
            "'builtin', ".repeat(builtin_themes::COUNT)
        )
    );
    assert_eq!(lines[1], "['Poster', 'Fig A']");
}

// --- Same-name conflicts (#546 T7): fixed order and a warning ----------------------
// Project, then user, then built-in; the script is never stopped.

fn stderr_of(b: &Bench, source: &str) -> String {
    let out = b.engine.run(source).unwrap();
    assert!(out.error.is_none(), "{:?}", out.error);
    out.stderr
}

#[test]
fn conflict_policy_the_project_theme_wins_over_the_user_one_with_one_warning() {
    let b = bench_with(vec![named("Fig A", 5.0)]);
    b.engine
        .set_theme_library(Some(library(&[], vec![named("Fig A", 6.0)])));
    let before = plot_graph(&b);
    let stderr = stderr_of(&b, "pyst.apply_theme(g, 'fig a')");
    assert_eq!(
        graph_spec(&b),
        expected(&b, &before, &named("Fig A", 5.0), default_families(), false)
    );
    assert_eq!(stderr.matches("ignored").count(), 1, "{stderr}");
    assert!(
        stderr.contains("Theme 'fig a': project:Fig A applied, user:Fig A ignored"),
        "{stderr}"
    );
    // It shows again from the same line of a loop.
    let again = stderr_of(&b, "for _ in range(2):\n    pyst.apply_theme(g, 'Fig A')");
    assert_eq!(again.matches("ignored").count(), 2, "{again}");
}

#[test]
fn conflict_policy_a_user_theme_wins_over_a_built_in_in_any_language() {
    let b = bench();
    b.engine.set_theme_library(Some(library(
        &[("light", "Clair")],
        vec![named("Clair", 6.0)],
    )));
    let before = plot_graph(&b);
    let stderr = stderr_of(&b, "pyst.apply_theme(g, 'Clair')");
    assert_eq!(
        graph_spec(&b),
        expected(&b, &before, &named("Clair", 6.0), default_families(), false)
    );
    assert!(
        stderr.contains("user:Clair applied, builtin:light ignored"),
        "{stderr}"
    );
}

#[test]
fn conflict_policy_a_prefix_settles_it_without_a_warning() {
    let b = bench_with(vec![named("Fig A", 5.0)]);
    b.engine
        .set_theme_library(Some(library(&[], vec![named("Fig A", 6.0)])));
    let before = plot_graph(&b);
    let stderr = stderr_of(&b, "pyst.apply_theme(g, 'user:Fig A')");
    assert_eq!(
        graph_spec(&b),
        expected(&b, &before, &named("Fig A", 6.0), default_families(), false)
    );
    assert!(stderr.is_empty(), "{stderr}");
    let stderr = stderr_of(&b, "pyst.apply_theme(g, 'project:fig a')");
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn conflict_policy_two_themes_of_one_source_take_the_first_and_warn() {
    let b = bench();
    b.engine.set_theme_library(Some(library(
        &[],
        vec![named("Twin", 5.0), named("twin", 6.0)],
    )));
    let before = plot_graph(&b);
    let stderr = stderr_of(&b, "pyst.apply_theme(g, 'user:Twin')");
    assert_eq!(
        graph_spec(&b),
        expected(&b, &before, &named("Twin", 5.0), default_families(), false)
    );
    assert!(stderr.contains("ignored"), "{stderr}");
}
