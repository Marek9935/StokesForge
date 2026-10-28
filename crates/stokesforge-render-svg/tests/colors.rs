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
