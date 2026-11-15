//! #529 (A5): the first and the last number of a horizontal axis are never cut
//! by the figure edge; the figure grows its outer margin just enough, and only
//! when a tick sits at the edge of a written range.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_render::sample;
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::{export_bytes, figure_size, ExportFormat, FontChoice, SvgTextMode};

/// The minimal graph, 600 wide, its bottom axis written `low..high`.
fn export(low: f64, high: f64) -> ((u32, u32), String) {
    let mut sample = sample::minimal().expect("minimal graph");
    sample.spec.title.clear();
    sample.spec.frame_widths = vec![600.0];
    sample.spec.frame_heights = vec![200.0];
    let axis = &mut sample.spec.panels.get_mut(0).expect("panel").bottom;
    axis.auto_range = false;
    axis.range_min = Some(low);
    axis.range_max = Some(high);
    let plot = resolve_spec(sample.project.tree(), &sample.spec);
    let (measurer, choice) = (TextMeasurer::default(), FontChoice::default());
    let size = figure_size(&sample.spec, &measurer, &choice);
    let svg = export_bytes(
        &plot,
        size,
        &measurer,
        &choice,
        ExportFormat::Svg {
            text: SvgTextMode::Text,
        },
    )
    .expect("SVG");
    (size, String::from_utf8(svg).expect("utf-8"))
}

/// The `x` of the `<text>` element whose content is `content`.
fn text_x(svg: &str, content: &str) -> f64 {
    for (start, _) in svg.match_indices("<text ") {
        let open_end = start + svg[start..].find('>').expect("tag end");
        let close = open_end + svg[open_end..].find("</text>").expect("close");
        if svg[open_end + 1..close].trim() != content {
            continue;
        }
        let tag = &svg[start..open_end];
        let at = tag.find(" x=\"").expect("x") + 4;
        return tag[at..at + tag[at..].find('"').expect("end")]
            .parse()
            .expect("number");
    }
    panic!("no text {content}");
}

#[test]
fn the_last_number_at_the_edge_of_a_written_range_stays_inside_the_figure() {
    let (size, svg) = export(0.0, 1000.0);
    // 600 + the left axis (12 + 64) + the margin: 688 before the reserve.
    assert!(size.0 > 688, "the right margin grew: {size:?}");
    // Centred on its tick, at the right edge of the frame (x = 676).
    let x = text_x(&svg, "1000");
    // "1000" is four digits of about 8 px: its right end is inside.
    assert!(x + 13.5 < f64::from(size.0), "x {x}, width {}", size.0);
    assert!(size.0 < 688 + 30, "just enough, not a lot: {size:?}");
}

#[test]
fn a_range_whose_last_tick_is_not_at_the_edge_does_not_grow_the_figure() {
    let (size, _) = export(0.0, 1050.0);
    assert_eq!(size, (688, 276), "no growth");
}
