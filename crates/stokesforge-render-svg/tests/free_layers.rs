//! Stacking and per-layer background of a free figure (#398) in the export.

use stokesforge_core::compose;
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{FreeFrame, LayerPlacement, PanelSpec, Panels, PlotSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::{figure_layout, render_to_string, FontChoice};

fn free(background: &str, cells: &[(f64, f64)]) -> PlotSpec {
    let panels: Vec<PanelSpec> = cells
        .iter()
        .enumerate()
        .map(|(column, &(left, top))| PanelSpec {
            free_frame: Some(FreeFrame {
                left,
                top,
                width: 300.0,
                height: 200.0,
                stack: 0,
            }),
            ..PanelSpec::at(0, column)
        })
        .collect();
    PlotSpec {
        layer_placement: LayerPlacement::Free,
        background: background.to_string(),
        ..PlotSpec::empty("").with_panels(Panels::new(panels))
    }
}

fn export(spec: &PlotSpec) -> (String, (u32, u32)) {
    let measurer = TextMeasurer::default();
    let layout = figure_layout(spec, &measurer, &FontChoice::default()).expect("layout");
    let project = Project::new("Layers").expect("project");
    let plot = resolve_spec(project.tree(), spec);
    (
        render_to_string(&plot, layout.size, &measurer).expect("export"),
        layout.size,
    )
}

/// Number of rectangles of the SVG filled with `fill` (`#RRGGBB`).
fn fills(svg: &str, fill: &str) -> usize {
    svg.lines()
        .filter(|l| {
            l.contains("<rect")
                && l.to_lowercase()
                    .contains(&format!("fill=\"{}\"", fill.to_lowercase()))
        })
        .count()
}

const OVERLAP: [(f64, f64); 2] = [(0.0, 0.0), (150.0, 40.0)];
const APART: [(f64, f64); 2] = [(0.0, 0.0), (500.0, 0.0)];

fn has_rect(svg: &str, x: i32, y: i32, w: u32, h: u32, fill: &str) -> bool {
    svg.contains(&format!(
        "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" opacity=\"1\" fill=\"{fill}\""
    ))
}

/// Witness: "Auto always paints" would add a rectangle without overlap;
/// "Auto never paints" would remove the one over the overlap.
#[test]
fn auto_paints_the_front_cell_only_where_it_covers_another() {
    let (svg, _) = export(&free("#ffcc00", &OVERLAP));
    assert_eq!(fills(&svg, "#FFCC00"), 2, "the figure and the front cell");
    assert!(has_rect(&svg, 150, 40, 388, 276, "#FFCC00"));
    let (apart, _) = export(&free("#ffcc00", &APART));
    assert_eq!(fills(&apart, "#FFCC00"), 1, "the figure alone");
}

/// Without a graph color the Auto background is white, and a transparent
/// export stays transparent where nothing overlaps.
#[test]
fn auto_is_white_without_a_graph_color_and_nothing_without_overlap() {
    let (svg, _) = export(&free("#00000000", &OVERLAP));
    assert!(has_rect(&svg, 150, 40, 388, 276, "#FFFFFF"));
    let (apart, _) = export(&free("#00000000", &APART));
    assert_eq!(apart.matches("<rect").count(), 0, "still transparent");
}

#[test]
fn an_explicit_color_is_painted_even_without_overlap() {
    let mut spec = free("#00000000", &APART);
    assert!(compose::set_layer_background(&mut spec, 1, "#336699"));
    let (svg, _) = export(&spec);
    assert_eq!(fills(&svg, "#336699"), 1);
}

#[test]
fn bringing_the_first_cell_to_the_front_moves_the_painted_rectangle() {
    let mut spec = free("#ffcc00", &OVERLAP);
    assert!(compose::bring_to_front(&mut spec, 0));
    let (svg, _) = export(&spec);
    assert!(has_rect(&svg, 0, 0, 388, 276, "#FFCC00"));
    assert!(!has_rect(&svg, 150, 40, 388, 276, "#FFCC00"));
}
