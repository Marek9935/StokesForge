//! Graphic objects (issue #76) seen in the exported document: the pipeline
//! is shared between the screen and the export, so what is placed shows up in
//! the SVG, with its colors.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{PanelSpec, Panels, PlotSpec, ShapeKind, ShapeSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

fn export(shapes: Vec<ShapeSpec>, background: &str) -> String {
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom.auto_range = false;
    panel.bottom.range_min = Some(0.0);
    panel.bottom.range_max = Some(10.0);
    panel.left.auto_range = false;
    panel.left.range_min = Some(0.0);
    panel.left.range_max = Some(10.0);
    panel.shapes = shapes;
    let spec = PlotSpec {
        background: background.to_owned(),
        ..PlotSpec::empty("Objects")
    }
    .with_panels(Panels::new(vec![panel]));
    let project = Project::new("Objects").expect("project");
    let plot = resolve_spec(project.tree(), &spec);
    render_to_string(&plot, (400, 300), &TextMeasurer::default()).expect("export")
}

/// Stroke and fill of a rectangle, stroke of an arrow: their colors
/// are in the document; without objects, they are not (witness).
#[test]
fn objects_are_plotted_in_the_export_with_their_colors() {
    let objects = vec![
        ShapeSpec {
            line_color: "#aa3311".to_owned(),
            fill_color: "#336699".to_owned(),
            ..ShapeSpec::new(ShapeKind::Rectangle, 2.0, 2.0, 6.0, 5.0)
        },
        ShapeSpec {
            line_color: "#11aa33".to_owned(),
            ..ShapeSpec::new(ShapeKind::Arrow, 1.0, 8.0, 8.0, 9.0)
        },
        ShapeSpec::new(ShapeKind::Ellipse, 5.0, 5.0, 9.0, 8.0),
    ];
    let with = export(objects, "#ffffff").to_uppercase();
    for color in ["#AA3311", "#336699", "#11AA33"] {
        assert!(with.contains(color), "{color} missing from the document");
    }
    let without = export(Vec::new(), "#ffffff").to_uppercase();
    for color in ["#AA3311", "#336699", "#11AA33"] {
        assert!(!without.contains(color), "{color} without objects: witness");
    }
}

/// A stroke without a color takes the automatic ink: white on a dark background.
#[test]
fn an_automatic_stroke_follows_the_background_ink() {
    let line = || vec![ShapeSpec::new(ShapeKind::Line, 1.0, 1.0, 9.0, 9.0)];
    let dark = export(line(), "#101820");
    let light = export(line(), "#ffffff");
    // The number of white strokes grows with the line on a dark background.
    let without = export(Vec::new(), "#101820");
    let whites = |s: &str| s.to_uppercase().matches("#FFFFFF").count();
    assert!(whites(&dark) > whites(&without));
    let without_light = export(Vec::new(), "#ffffff");
    let ink = |s: &str| s.to_uppercase().matches("#202020").count();
    assert!(ink(&light) > ink(&without_light));
}

/// #91: the ellipse is plotted at subpixel precision. The document keeps no trace
/// of the internal convention (width flag, fixed points), and its
/// vertices are not all integers — otherwise the circle gets jagged again.
#[test]
fn an_ellipse_is_exported_at_subpixel_with_no_trace_of_the_convention() {
    let svg = export(
        vec![ShapeSpec::new(ShapeKind::Ellipse, 1.0, 1.0, 9.0, 9.0)],
        "#ffffff",
    );
    assert!(!svg.contains("stroke-width=\"1677"), "flag left behind");
    let fractional = svg
        .split("points=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .flat_map(str::split_whitespace)
        .flat_map(|pair| pair.split(','))
        .any(|value| value.contains('.'));
    assert!(fractional, "all the vertices are integers");
}
