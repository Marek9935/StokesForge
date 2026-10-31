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

/// #405: a Fixed figure with Left and Top is exported at the size of its
/// layout, in every format: the reserve of four axes (152 x 128 around the
/// frame) and Left = 150 in place of the 76 of the left axis.
#[test]
fn a_fixed_figure_with_left_and_top_is_exported_at_its_layout_size() {
    let mut sample = sample::minimal().expect("minimal graph");
    sample.spec.title.clear();
    sample.spec.frame_widths = vec![600.0];
    sample.spec.frame_heights = vec![200.0];
    sample.spec.fixed_figure = true;
    sample.spec.figure_left = Some(150.0);
    sample.spec.figure_top = Some(90.0);
    let plot = resolve_spec(sample.project.tree(), &sample.spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();

    let size = figure_size(&sample.spec, &measurer, &choice);
    // Left 150 + 600 + 76 (right reserve); Top 90 + 200 + 64 (bottom reserve).
    assert_eq!(size, (826, 354));

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
    assert!(svg.contains("width=\"826\"") && svg.contains("height=\"354\""));
    let pdf = export_bytes(&plot, size, &measurer, &choice, ExportFormat::Pdf).expect("PDF");
    assert_eq!(media_box(&pdf), vec![0.0, 0.0, 826.0, 354.0]);
    let png = export_bytes(
        &plot,
        size,
        &measurer,
        &choice,
        ExportFormat::Png { scale: 1.0 },
    )
    .expect("PNG");
    assert_eq!(png_size(&png), (826, 354));
}

/// #407: a title that wraps once the image is narrower grows the title band,
/// so the height is fitted after the width, on the figure as it then is.
/// Witness: the height bands measured before the width change (the image would
/// miss the target by the extra lines).
#[test]
fn fitting_the_height_after_the_width_measures_the_title_at_the_new_width() {
    use stokesforge_render::pipeline::SizeAxis;
    use stokesforge_render_svg::fit_figure_size;

    let mut sample = sample::minimal().expect("minimal graph");
    sample.spec.title = "A very long graph title that has to wrap over several lines".to_owned();
    sample.spec.title_visible = true;
    // A title box half the width of the image: the narrower, the more lines.
    sample.spec.title_font.width = Some(0.5);
    sample.spec.frame_widths = vec![600.0];
    sample.spec.frame_heights = vec![200.0];
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let band = |spec: &stokesforge_plot::PlotSpec| {
        stokesforge_render_svg::figure_layout(spec, &measurer, &choice)
            .expect("layout")
            .title_band
    };
    let fit = |spec: &mut stokesforge_plot::PlotSpec, axis, target| {
        let fitted = fit_figure_size(spec, &measurer, &choice, axis, target)
            .expect("measured")
            .expect("a fit");
        fitted.apply_to(spec);
    };

    let wide_band = band(&sample.spec);
    fit(&mut sample.spec, SizeAxis::Width, 260.0);
    assert!(band(&sample.spec) > wide_band, "the title wraps at 260 pt");
    fit(&mut sample.spec, SizeAxis::Height, 400.0);
    let size = figure_size(&sample.spec, &measurer, &choice);
    assert_eq!(size.1, 400);
    assert_eq!(size.0, 260);
}
