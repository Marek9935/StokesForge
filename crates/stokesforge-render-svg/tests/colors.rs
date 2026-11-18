//! Colors of the text and the scales, seen in the exported document: the
//! same rule as the screen, since the pipeline is shared.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{PanelSpec, Panels, PlotSpec};
use stokesforge_render::{FontChoice, TextMeasurer};
use stokesforge_render_svg::{render_to_string, render_to_string_with_typst, SvgTextMode};

fn export(spec: &PlotSpec) -> String {
    let project = Project::new("Colors").expect("project");
    let plot = resolve_spec(project.tree(), spec);
    render_to_string(&plot, (400, 300), &TextMeasurer::default()).expect("export")
}

fn graph(background: &str) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom.label = "Time".to_owned();
    panel.left.label = "Signal".to_owned();
    PlotSpec {
        background: background.to_owned(),
        ..PlotSpec::empty("Title")
    }
    .with_panels(Panels::new(vec![panel]))
}

/// On a dark background, everything in automatic color turns white: title,
/// axis titles, ticks. On a light background, nothing is white.
#[test]
fn a_dark_background_turns_the_automatic_text_white() {
    let dark = export(&graph("#101820")).to_uppercase();
    assert!(
        dark.contains("#FFFFFF"),
        "white text expected on a dark background"
    );
    assert!(
        !dark.contains("#202020"),
        "no more dark ink on a dark background"
    );

    let light = export(&graph("#ffffff")).to_uppercase();
    assert!(light.contains("#202020"), "dark ink on a light background");
}

/// An explicit color takes precedence over the automatic ink.
#[test]
fn an_explicit_color_takes_precedence_over_the_automatic_one() {
    let mut spec = graph("#ffffff");
    spec.title_color = "#aa3311".to_owned();
    if let Some(panel) = spec.panels.get_mut(0) {
        panel.bottom.title_color = "#1177cc".to_owned();
        panel.left.tick_color = "#22aa44".to_owned();
    }
    let document = export(&spec).to_uppercase();
    for color in ["#AA3311", "#1177CC", "#22AA44"] {
        assert!(
            document.contains(color),
            "{color} missing from the document"
        );
    }
}

/// The title's Typst fragment, as `splice` inlined it: from the nested
/// `<svg>` (the first one after the root: the graph title is plotted
/// before the axis titles and the legend) to the end of its group.
/// `document` is uppercased by the caller, to compare colors
/// case-insensitively.
fn typst_fragment(document: &str) -> &str {
    let root = document.find("<SVG").expect("empty document") + 1;
    let start = root
        + document[root..]
            .find("<SVG WIDTH=")
            .expect("no Typst fragment");
    let end = document[start..]
        .find("</SVG></G>")
        .expect("Typst fragment not closed");
    &document[start..start + end]
}

fn export_with_typst(spec: &PlotSpec) -> String {
    let project = Project::new("Colors").expect("project");
    let plot = resolve_spec(project.tree(), spec);
    render_to_string_with_typst(
        &plot,
        (400, 300),
        &TextMeasurer::default(),
        &FontChoice::default(),
        SvgTextMode::Text,
    )
    .expect("export")
}

/// Issue #30: a Typst title takes the text color. On a dark
/// background, the automatic ink is white, and the Typst fragment
/// itself carries it — before, it was always black. The witness is the
/// same title on a light background: its fragment is not white.
#[test]
fn a_typst_title_takes_the_automatic_ink() {
    let title = |background: &str| PlotSpec {
        title: "$lambda$ (nm)".to_owned(),
        ..graph(background)
    };
    let dark = export_with_typst(&title("#101820")).to_uppercase();
    let fragment = typst_fragment(&dark);
    assert!(
        fragment.contains("#FFFFFF"),
        "Typst title not white:\n{fragment}"
    );
    assert!(!fragment.contains("#000000"), "Typst title still black");

    let light = export_with_typst(&title("#ffffff")).to_uppercase();
    let fragment = typst_fragment(&light);
    assert!(!fragment.contains("#FFFFFF"), "the light witness is white");
    assert!(fragment.contains("#202020"), "dark ink expected");
}

/// An explicit title color goes into the Typst fragment, and a transparent
/// title draws nothing — neither text nor fragment.
#[test]
fn a_typst_title_follows_title_color() {
    let mut spec = PlotSpec {
        title: "$Delta H$".to_owned(),
        ..graph("#ffffff")
    };
    spec.title_color = "#aa3311".to_owned();
    let document = export_with_typst(&spec).to_uppercase();
    assert!(typst_fragment(&document).contains("#AA3311"));

    let opaque = document.matches("<SVG").count();
    spec.title_color = "#aa331100".to_owned();
    let document = export_with_typst(&spec);
    assert_eq!(
        document.matches("<svg").count(),
        opaque - 1,
        "a transparent title produced a Typst fragment"
    );
}

// -- Graph-level colours (#550) ---------------------------------------------

/// A light graph with one kind of text only, in the graph-level colour
/// `#123456` (read in the uppercased export).
fn only_title() -> PlotSpec {
    let mut spec = graph("#ffffff");
    spec.panels.get_mut(0).unwrap().bottom.label.clear();
    spec.panels.get_mut(0).unwrap().left.label.clear();
    spec.text_color = "#123456".to_owned();
    spec
}

#[test]
fn the_graph_text_color_draws_the_graph_title_left_on_automatic() {
    let document = export_with_typst(&only_title()).to_uppercase();
    assert!(typst_fragment(&document).contains("#123456"));
}

#[test]
fn the_graph_text_color_draws_the_axis_titles_left_on_automatic() {
    let mut spec = only_title();
    spec.title.clear();
    spec.panels.get_mut(0).unwrap().bottom.label = "Time".to_owned();
    spec.panels.get_mut(0).unwrap().left.label = "Signal".to_owned();
    let document = export_with_typst(&spec).to_uppercase();
    assert!(document.contains("#123456"), "axis titles not recoloured");
    spec.text_color.clear();
    let document = export_with_typst(&spec).to_uppercase();
    assert!(!document.contains("#123456"));
}

#[test]
fn the_graph_text_color_draws_the_free_texts_left_on_automatic() {
    let mut spec = only_title();
    spec.title.clear();
    spec.panels
        .get_mut(0)
        .unwrap()
        .annotations
        .push(stokesforge_plot::AnnotationSpec::new("Note", 0.5, 0.5));
    let document = export_with_typst(&spec).to_uppercase();
    assert!(document.contains("#123456"), "free text not recoloured");
    spec.panels.get_mut(0).unwrap().annotations.clear();
    let document = export_with_typst(&spec).to_uppercase();
    assert!(!document.contains("#123456"));
}

#[test]
fn the_graph_axes_color_draws_the_ticks_and_labels_left_on_automatic() {
    let mut spec = graph("#ffffff");
    spec.axes_color = "#654321".to_owned();
    let document = export(&spec).to_uppercase();
    assert!(document.contains("#654321"), "scales not recoloured");
    spec.axes_color.clear();
    assert!(!export(&spec).to_uppercase().contains("#654321"));
}

#[test]
fn a_local_color_beats_the_graph_color_and_the_rest_follows_it() {
    let mut spec = graph("#ffffff");
    spec.text_color = "#123456".to_owned();
    spec.axes_color = "#654321".to_owned();
    spec.panels.get_mut(0).unwrap().left.title_color = "#ff0000".to_owned();
    spec.panels.get_mut(0).unwrap().bottom.tick_color = "#00ff00".to_owned();
    let document = export_with_typst(&spec).to_uppercase();
    for color in ["#FF0000", "#00FF00", "#123456", "#654321"] {
        assert!(document.contains(color), "{color} missing");
    }
}

#[test]
fn an_unreadable_graph_color_falls_back_to_the_automatic_ink() {
    let mut spec = graph("#ffffff");
    spec.text_color = "#zz".to_owned();
    spec.axes_color = "nope".to_owned();
    let document = export_with_typst(&spec).to_uppercase();
    assert!(document.contains("#202020"), "automatic ink expected");
    assert!(!document.contains("#ZZ"));
}

#[test]
fn a_graph_color_applies_on_a_layer_with_a_dark_background() {
    let mut spec = graph("#ffffff");
    spec.panels.get_mut(0).unwrap().layer_background = "#101820".to_owned();
    spec.axes_color = "#654321".to_owned();
    let document = export(&spec).to_uppercase();
    assert!(document.contains("#654321"));
}

// -- Graph text colour on every text (#550, G1) ---------------------------

/// A one-layer graph with a visible layer title and a legend entry, and the
/// graph text colour set; nothing else carries text.
fn with_layer_title_and_legend(adjust: impl FnOnce(&mut PlotSpec)) -> String {
    use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
    use stokesforge_plot::SeriesSpec;
    let mut project = Project::new("Colors").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let mut add = |name: &str| {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(vec![0.0, 1.0, 2.0]),
            ColumnMeta::default(),
        )
        .expect("column");
        project
            .add_column(sheet, None, Some(name), column)
            .expect("column")
    };
    let (x, y) = (add("X"), add("Signal"));
    let mut spec = graph("#ffffff");
    spec.panels
        .get_mut(0)
        .unwrap()
        .series
        .push(SeriesSpec::new_xy(x, y));
    adjust(&mut spec);
    let plot = resolve_spec(project.tree(), &spec);
    render_to_string_with_typst(
        &plot,
        (400, 300),
        &TextMeasurer::default(),
        &FontChoice::default(),
        SvgTextMode::Text,
    )
    .expect("export")
    .to_uppercase()
}

#[test]
fn the_graph_text_color_draws_the_layer_title_left_on_automatic() {
    let set = |spec: &mut PlotSpec| {
        spec.title.clear();
        let panel = spec.panels.get_mut(0).unwrap();
        panel.title = "Layer".to_owned();
        panel.title_visible = true;
        panel.legend.visible = false;
        panel.bottom.label.clear();
        panel.left.label.clear();
    };
    let on = with_layer_title_and_legend(|spec| {
        set(spec);
        spec.text_color = "#123456".to_owned();
    });
    assert!(on.contains("#123456"), "layer title not recoloured");
    let off = with_layer_title_and_legend(set);
    assert!(!off.contains("#123456"));
}

#[test]
fn the_graph_text_color_draws_the_legend_text_left_on_automatic() {
    let set = |spec: &mut PlotSpec| {
        spec.title.clear();
        let panel = spec.panels.get_mut(0).unwrap();
        panel.bottom.label.clear();
        panel.left.label.clear();
        panel.legend.visible = true;
    };
    let on = with_layer_title_and_legend(|spec| {
        set(spec);
        spec.text_color = "#123456".to_owned();
    });
    assert!(on.contains("#123456"), "legend text not recoloured");
    let off = with_layer_title_and_legend(set);
    assert!(!off.contains("#123456"));
}

#[test]
fn an_own_legend_or_layer_title_color_beats_the_graph_text_color() {
    let document = with_layer_title_and_legend(|spec| {
        spec.text_color = "#123456".to_owned();
        let panel = spec.panels.get_mut(0).unwrap();
        panel.title = "Layer".to_owned();
        panel.title_visible = true;
        panel.title_color = "#ff0000".to_owned();
        panel.legend.color = "#00ff00".to_owned();
    });
    assert!(document.contains("#FF0000") && document.contains("#00FF00"));
}
