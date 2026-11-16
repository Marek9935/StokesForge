//! #546: `themed_spec` (moved here from the application state) gives the spec
//! the application used to build: the pure copy of the families, with the
//! missing layers added on request.

use stokesforge_core::Project;
use stokesforge_plot::builtin_themes;
use stokesforge_plot::{PlotSpec, ThemeFamilies};
use stokesforge_render_svg::{themed_spec, FigureTexts};

fn publication() -> stokesforge_plot::GraphTheme {
    builtin_themes::original(builtin_themes::index_of("publication").expect("key"))
        .expect("built-in")
}

#[test]
fn publication_on_a_fresh_graph_is_the_pure_copy_of_every_family() {
    let project = Project::new("T").expect("project");
    let current = PlotSpec::empty("Graph1");
    let theme = publication();
    let got = themed_spec(
        project.tree(),
        &current,
        &theme,
        ThemeFamilies::all(),
        true,
        &FigureTexts::default(),
    );
    let mut expected = current.clone();
    expected.apply_theme_families(&theme, ThemeFamilies::all());
    assert_eq!(got, expected);
    assert_ne!(got, current, "the theme changed something");
    assert_eq!(current, PlotSpec::empty("Graph1"), "the input is untouched");
}

#[test]
fn missing_layers_are_added_only_on_request() {
    let project = Project::new("T").expect("project");
    let current = PlotSpec::empty("Graph1");
    let theme = builtin_themes::original(builtin_themes::index_of("fit-residuals").expect("key"))
        .expect("built-in");
    let run = |add| {
        themed_spec(
            project.tree(),
            &current,
            &theme,
            ThemeFamilies::all(),
            add,
            &FigureTexts::default(),
        )
    };
    assert_eq!(run(false).panels.len(), 1);
    let stacked = run(true);
    assert_eq!(stacked.panels.len(), 2);
    assert_eq!(stacked.panels.as_slice()[1].row, 1);
}
