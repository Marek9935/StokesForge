//! #394: the page of an export is the figure's own size (frames plus bands)
//! (`stokesforge_render_svg::figure_size`), whatever the format: the SVG
//! `width`/`height`, the PDF `MediaBox`, the PNG pixels at the scale.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_render::sample;
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::{export_bytes, figure_size, ExportFormat, FontChoice, SvgTextMode};

fn media_box(pdf: &[u8]) -> Vec<f32> {
    let document = lopdf::Document::load_mem(pdf).expect("the PDF opens");
    let (_, id) = document.get_pages().into_iter().next().expect("one page");
    let page = document.get_dictionary(id).expect("page");
    page.get(b"MediaBox")
        .expect("MediaBox")
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_float().expect("numeric"))
        .collect()
}

/// The dimensions of a PNG, read from its IHDR chunk.
fn png_size(png: &[u8]) -> (u32, u32) {
    let be = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().expect("4 bytes"));
    (be(16), be(20))
}

#[test]
fn every_export_format_uses_the_size_of_the_frames_plus_their_bands() {
    let mut sample = sample::minimal().expect("minimal graph");
    sample.spec.title.clear();
    sample.spec.frame_widths = vec![600.0];
    sample.spec.frame_heights = vec![200.0];
    let plot = resolve_spec(sample.project.tree(), &sample.spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();

    let size = figure_size(&sample.spec, &measurer, &choice);
    // The frame is 600 x 200; two margins and the left axis (24 + 64) beside
    // it, two margins and the bottom axis (24 + 52) below.
    assert_eq!(size, (688, 276), "not the 480 x 360 default");

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
    let svg = String::from_utf8(svg).expect("utf-8");
    assert!(svg.contains("width=\"688\"") && svg.contains("height=\"276\""));

    let pdf = export_bytes(&plot, size, &measurer, &choice, ExportFormat::Pdf).expect("PDF");
    assert_eq!(media_box(&pdf), vec![0.0, 0.0, 688.0, 276.0]);

    let png = export_bytes(
        &plot,
        size,
        &measurer,
        &choice,
        ExportFormat::Png { scale: 2.0 },
    )
    .expect("PNG");
    assert_eq!(png_size(&png), (1376, 552));
}

#[test]
fn a_graph_without_stored_frames_keeps_the_legacy_page() {
    let mut sample = sample::minimal().expect("minimal graph");
    sample.spec.title.clear();
    assert_eq!(
        figure_size(
            &sample.spec,
            &TextMeasurer::default(),
            &FontChoice::default()
        ),
        (480, 360)
    );
}
