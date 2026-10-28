//! Task R6.2: the PDF opens, and its dimensions match those of the
//! SVG it comes from.
//!
//! "Opens" means: a PDF parser independent of ours — `lopdf`,
//! which shares no code with `svg2pdf` — re-parses the document without
//! error. A mere byte search in our own output would prove nothing more
//! than "`svg2pdf` wrote the bytes we asked it to write"; `lopdf` proves
//! that the PDF structure itself (object table, page dictionary, `MediaBox`)
//! is valid.
//!
//! "Its dimensions match" is checked against three sources, not just
//! one: the size requested at export, the size actually written in the
//! SVG's `width`/`height` attribute, and the `MediaBox` that `lopdf` finds
//! in the PDF. Comparing only against the first two would not see a
//! divergence between [`render_to_string`] and [`render_to_pdf`] — two
//! separate calls into the pipeline, even if they share all their code today.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_render::sample;
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::{render_to_pdf, render_to_pdf_file, render_to_string};

/// Extracts the numeric value of a `name="value"` attribute of an SVG
/// document. Panics if absent: in this file, its absence is a test
/// failure, not a case to tolerate.
fn numeric_attribute(document: &str, name: &str) -> f64 {
    let pattern = format!("{name}=\"");
    let start = document
        .find(&pattern)
        .unwrap_or_else(|| panic!("attribute `{name}` missing from the SVG"))
        + pattern.len();
    let rest = &document[start..];
    let end = rest
        .find('"')
        .unwrap_or_else(|| panic!("attribute `{name}` not closed"));
    rest[..end]
        .parse()
        .unwrap_or_else(|err| panic!("attribute `{name}` not numeric: {err}"))
}

/// The `MediaBox` of the first page of a PDF, `(width, height)` in
/// points — read by `lopdf`, independently of `svg2pdf`.
fn media_box(pdf: &[u8]) -> (f64, f64) {
    let document = lopdf::Document::load_mem(pdf).expect("the PDF must open without error");
    let pages = document.get_pages();
    assert_eq!(pages.len(), 1, "an exported figure = one PDF page");
    let (_, id) = pages.into_iter().next().expect("at least one page");
    let page = document
        .get_dictionary(id)
        .expect("page dictionary not found");
    let media_box = page
        .get(b"MediaBox")
        .expect("MediaBox missing from the page")
        .as_array()
        .expect("MediaBox is not an array");
    let value = |i: usize| {
        media_box[i]
            .as_float()
            .unwrap_or_else(|_| panic!("MediaBox[{i}] is not numeric")) as f64
    };
    (value(2) - value(0), value(3) - value(1))
}

#[test]
fn the_pdf_opens_and_its_dimensions_match_the_svg() {
    // The realistic graph of R5: five panels, six plot types, logarithmic
    // scale — not an empty canvas that would agree trivially.
    let sample = sample::realistic().expect("realistic graph");
    let plot = resolve_spec(sample.project.tree(), &sample.spec);
    let measurer = TextMeasurer::default();
    let size = (1280, 860);

    let document = render_to_string(&plot, size, &measurer).expect("SVG export");
    let pdf = render_to_pdf(&plot, size, &measurer).expect("PDF export");

    assert!(
        pdf.len() > 2_000,
        "suspiciously short PDF ({} bytes) for five panels",
        pdf.len()
    );

    let (svg_width, svg_height) = (
        numeric_attribute(&document, "width"),
        numeric_attribute(&document, "height"),
    );
    let (pdf_width, pdf_height) = media_box(&pdf);

    // Three sources, a single number expected on each side: the requested
    // size, the one actually written in the SVG, and the one that `lopdf`
    // finds in the PDF — with `PageOptions::default()` (72 DPI), the SVG's
    // user units and the PDF points coincide one for one.
    assert_eq!(svg_width, size.0 as f64, "SVG width ≠ requested width");
    assert_eq!(svg_height, size.1 as f64, "SVG height ≠ requested height");
    assert_eq!(
        pdf_width, svg_width,
        "PDF width ({pdf_width}) ≠ SVG width ({svg_width})"
    );
    assert_eq!(
        pdf_height, svg_height,
        "PDF height ({pdf_height}) ≠ SVG height ({svg_height})"
    );
}

#[test]
fn another_figure_size_gives_another_mediabox() {
    // Witness: if `media_box` always returned the same thing (for example
    // because `render_to_pdf` ignored `size`), the previous test would pass
    // for the wrong reason. The minimal graph is enough here.
    let sample = sample::minimal().expect("minimal graph");
    let plot = resolve_spec(sample.project.tree(), &sample.spec);
    let measurer = TextMeasurer::default();

    let small = render_to_pdf(&plot, (400, 300), &measurer).expect("PDF export (400×300)");
    let large = render_to_pdf(&plot, (900, 600), &measurer).expect("PDF export (900×600)");

    assert_eq!(media_box(&small), (400.0, 300.0));
    assert_eq!(media_box(&large), (900.0, 600.0));
    assert_ne!(media_box(&small), media_box(&large));
}

#[test]
fn pdf_writing_to_disk() {
    let sample = sample::minimal().expect("minimal graph");
    let plot = resolve_spec(sample.project.tree(), &sample.spec);
    let dir = tempfile::tempdir().expect("temporary directory");
    let target = dir.path().join("figure.pdf");

    render_to_pdf_file(&target, &plot, (640, 480), &TextMeasurer::default()).expect("write");

    let content = std::fs::read(&target).expect("read back");
    assert!(content.starts_with(b"%PDF-"), "PDF header missing");
    assert_eq!(media_box(&content), (640.0, 480.0));
}
