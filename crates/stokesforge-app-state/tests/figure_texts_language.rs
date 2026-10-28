//! The texts a figure draws by itself follow the interface language (#369).
//!
//! One test, in its own binary: the language is global, and two tests that
//! switch it in parallel would trample each other.

use stokesforge_app_state::figure_texts::{figure_texts, theme_name};
use stokesforge_app_state::i18n::{self, build_loader, Language};
use stokesforge_app_state::object_manager::object_tree;
use stokesforge_app_state::state::{series_list_label, Action, AppState};
use stokesforge_app_state::tr;
use stokesforge_core::column::Column;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_session::Session;

/// A graph with an unnamed and a named series whose Y column is gone.
fn broken_graph() -> (Project, NodeId) {
    let mut project = Project::new("Texts").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let mut panel = PanelSpec::at(0, 0);
    for name in ["", "N"] {
        let id = project
            .add_column(sheet, None, Some("Gone"), Column::from_f64(&[1.0, 2.0]))
            .expect("column");
        let mut series = SeriesSpec::new(id);
        series.name = name.to_owned();
        panel.series.push(series);
        project.remove(id).expect("column removed");
    }
    let spec = PlotSpec::empty("Graph").with_panels(panel);
    let graph = project
        .add_graph(root, Some("Graph1"), spec)
        .expect("graph");
    (project, graph)
}

fn spec_of(state: &AppState, graph: NodeId) -> PlotSpec {
    match state.tree().get(graph).expect("graph").kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("not a graph"),
    }
}

#[test]
fn the_broken_reference_texts_follow_the_interface_language() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (project, graph) = broken_graph();
    let session = Session::spawn(project, None).expect("session");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .expect("state");
    let spec = spec_of(&state, graph);

    for (language, placeholder, export) in [
        (Language::English, "(broken reference)", "broken reference"),
        (Language::French, "(référence rompue)", "référence rompue"),
    ] {
        i18n::set_language(language);

        // The texts handed to the renderer, and the theme names.
        let texts = figure_texts();
        assert_eq!(texts.language, language.tag());
        assert_eq!(texts.broken_reference, placeholder);
        if language == Language::French {
            assert_eq!(texts.broken_named("N"), "N — référence rompue");
            assert_eq!(texts.more_entries(1), "… et 1 autre");
            assert_eq!(texts.more_entries(3), "… et 3 autres");
            assert_eq!(texts.row_series(2), "Ligne 2");
            assert_eq!(
                (theme_name(0), theme_name(1)),
                ("Clair".into(), "Sombre".into())
            );
        } else {
            assert_eq!(texts.more_entries(3), "… and 3 more");
            assert_eq!(texts.row_series(2), "Row 2");
            assert_eq!(
                (theme_name(0), theme_name(1)),
                ("Light".into(), "Dark".into())
            );
        }

        // The series list and the Object Manager phrase the placeholder.
        assert_eq!(
            series_list_label(state.tree(), &spec.panels[0].series[0]),
            placeholder
        );
        let tree = object_tree(state.tree(), graph, None).expect("object tree");
        assert_eq!(tree.layers[0].series[0].label, placeholder);

        // The export draws it in the same language.
        let path = dir.path().join(format!("{}.svg", language.tag()));
        state.dispatch(Action::ExportGraph {
            path: path.clone(),
            spec: Box::new(spec.clone()),
        });
        // Typst lays the legend out glyph by glyph: the spaces are not in the
        // file, so the comparison ignores them.
        let svg: String = std::fs::read_to_string(&path)
            .expect("exported file")
            .split_whitespace()
            .collect();
        let squeeze = |text: &str| text.replace(' ', "");
        let (other, other_named) = if language == Language::English {
            ("référence rompue", "N—référencerompue")
        } else {
            ("broken reference", "N—brokenreference")
        };
        assert!(
            svg.contains(&squeeze(export)),
            "{language:?}: {export:?} not in the export"
        );
        assert!(
            svg.contains(&format!("N—{}", squeeze(export))),
            "{language:?}: the named series is not annotated in the export"
        );
        assert!(
            !svg.contains(&squeeze(other)) && !svg.contains(other_named),
            "{language:?}: {other:?} leaked into the export"
        );
    }

    // The import failure sentences are catalog texts, not literals.
    for (language, close, book, sheet) in [
        (
            Language::English,
            "the import history entry could not be closed",
            "A: the workbook could not be created",
            "A: the sheet could not be created",
        ),
        (
            Language::French,
            "l'entrée d'historique de l'import n'a pas pu être fermée",
            "A : le classeur n'a pas pu être créé",
            "A : la feuille n'a pas pu être créée",
        ),
    ] {
        let loader = build_loader(language);
        assert_eq!(tr!(in &loader; "error-import-close-group"), close);
        assert_eq!(
            tr!(in &loader; "error-import-create-book", name = "A"),
            book
        );
        assert_eq!(
            tr!(in &loader; "error-import-create-sheet", name = "A"),
            sheet
        );
    }

    i18n::set_language(Language::English);
}
