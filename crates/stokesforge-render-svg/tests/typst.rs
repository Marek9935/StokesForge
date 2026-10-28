//! Typst export path — task R8.4, and the export counterpart of
//! invariant 8.5.
//!
//! Compiled permanently since issue #29 (no more `typst` feature),
//! like `stokesforge-render-egui/tests/typst.rs`.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{PanelSpec, Panels, PlotSpec};
use stokesforge_render::{FontChoice, TextMeasurer};
use stokesforge_render_svg::{render_to_string, render_to_string_with_typst};

fn spec_with_label(label: &str) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.left.label = label.to_owned();
    panel.bottom.label = "t (s)".to_owned();
    PlotSpec::empty("").with_panels(Panels::new(vec![panel]))
}

/// **The export counterpart of the most important test of the phase**: the
/// document produced with Typst enabled contains an inlined `typst-svg`
/// fragment for the formula label, and no more witness — proof that
/// `splice` did its job, not just that `estimate_text_size` changed
/// value.
#[test]
fn the_typst_document_contains_an_inline_fragment_with_no_residual_witness() {
    let project = Project::new("Typst export").expect("project");
    let spec = spec_with_label("$lambda$ (nm)");
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();

    let without_typst =
        render_to_string(&plot, (400, 300), &measurer).expect("export without Typst failed");
    let with_typst = render_to_string_with_typst(
        &plot,
        (400, 300),
        &measurer,
        &choice,
        stokesforge_render_svg::SvgTextMode::Outlines,
    )
    .expect("export with Typst failed");

    // The document remains a valid, non-empty SVG, in both cases.
    assert!(without_typst.trim_start().starts_with("<svg"));
    assert!(with_typst.trim_start().starts_with("<svg"));

    // The inlined Typst fragment is present: a **nested** `<svg>`, in
    // addition to the root tag that `SVGBackend` writes anyway in
    // both documents (hence the comparison by count, not by mere
    // presence).
    let count = |document: &str| document.matches("<svg").count();
    assert_eq!(
        count(&without_typst),
        1,
        "unexpected document without Typst"
    );
    assert!(
        count(&with_typst) >= 2,
        "no nested <svg> found in the Typst document:\n{with_typst}"
    );

    // No witness remains in the final document — `splice` replaced
    // all the witness rectangles it placed.
    assert!(
        !with_typst.contains("fill=\"#010203\""),
        "a Typst witness was not substituted:\n{with_typst}"
    );

    // The two documents are not identical: the plotted geometry
    // differs (width of the left tick area, measured by
    // Typst rather than by the `epaint` heuristic).
    assert_ne!(without_typst, with_typst);
}

/// The path without Typst is not affected by the existence of the feature:
/// [`render_to_string`] returns the same result as a call that would ignore
/// any formula label (no regression of the default path).
#[test]
fn render_to_string_ignores_the_formula_syntax_without_the_typst_call() {
    let project = Project::new("Typst export").expect("project");
    let spec = spec_with_label("$lambda$ (nm)");
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();

    let document = render_to_string(&plot, (400, 300), &measurer).expect("export failed");
    assert!(
        document.contains("$lambda$"),
        "the source text must stay literal"
    );
    assert_eq!(
        document.matches("<svg").count(),
        1,
        "no nested <svg> must appear without the Typst call"
    );
}

/// Issue #34: a graph with Typst text exports to SVG, PDF and PNG. The PDF and
/// the PNG are derived from the SVG by `usvg`, which rejected the document as long as the
/// Typst fragment used `xlink:href` without declaring the prefix — the
/// defect this test would have seen. And the figure's text is no longer raw
/// `<text>`: no `$` remains in it, it is in Typst outlines.
#[test]
fn a_typst_graph_exports_to_svg_pdf_and_png() {
    use stokesforge_render_svg::{export_bytes, ExportFormat};
    let project = Project::new("Typst export").expect("project");
    let mut spec = spec_with_label("$lambda$ (nm)");
    spec.title = "Spectrum $Delta H$".to_owned();
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let export = |format| {
        export_bytes(&plot, (480, 360), &measurer, &choice, format)
            .unwrap_or_else(|error| panic!("export {format:?}: {error}"))
    };

    let svg = String::from_utf8(export(ExportFormat::Svg {
        text: stokesforge_render_svg::SvgTextMode::Text,
    }))
    .expect("SVG in UTF-8");
    for (start, _) in svg.match_indices("<text") {
        let end = svg[start..]
            .find("</text>")
            .map_or(svg.len(), |i| start + i);
        assert!(
            !svg[start..end].contains('$'),
            "raw dollar text in the export: {}",
            &svg[start..end]
        );
    }
    assert!(svg.contains("xmlns:xlink"), "xlink prefix not declared");

    let pdf = export(ExportFormat::Pdf);
    assert!(pdf.starts_with(b"%PDF"), "not a PDF");
    // The witness: the same graph without Typst gives another PDF.
    let raw = stokesforge_render_svg::render_to_pdf(&plot, (480, 360), &measurer).expect("raw PDF");
    assert_ne!(pdf, raw, "the PDF owes nothing to Typst");

    let png = export(ExportFormat::Png { scale: 2.0 });
    assert!(png.starts_with(b"\x89PNG"), "not a PNG");
    // Width in the IHDR header (bytes 16 to 19, big-endian).
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    assert_eq!(width, 960, "PNG not at scale 2");
}

/// #149: the grouped export gives, format by format, the same bytes as
/// the isolated export — it only avoids composing the figure three times.
/// Witnesses: the requested order is the returned order (an SVG is not a PNG), and
/// an empty list returns nothing.
#[test]
fn the_grouped_export_returns_the_same_bytes_as_the_isolated_export() {
    use stokesforge_render_svg::{export_bytes, export_bytes_many, ExportFormat};
    let project = Project::new("Grouped export").expect("project");
    let mut spec = spec_with_label("$lambda$ (nm)");
    spec.title = "Spectrum $Delta H$".to_owned();
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let formats = [
        ExportFormat::Png { scale: 1.0 },
        ExportFormat::Svg {
            text: stokesforge_render_svg::SvgTextMode::Text,
        },
        ExportFormat::Pdf,
    ];
    let grouped = export_bytes_many(&plot, (480, 360), &measurer, &choice, &formats)
        .expect("drawing")
        .into_iter()
        .map(|bytes| bytes.expect("conversion"))
        .collect::<Vec<_>>();
    assert_eq!(grouped.len(), 3);
    assert!(grouped[0].starts_with(b"\x89PNG"), "first: PNG");
    assert!(grouped[1].starts_with(b"<?xml") || grouped[1].starts_with(b"<svg"));
    assert!(grouped[2].starts_with(b"%PDF"), "third: PDF");
    for (format, bytes) in formats.iter().zip(&grouped) {
        let isolated =
            export_bytes(&plot, (480, 360), &measurer, &choice, *format).expect("export");
        if matches!(format, ExportFormat::Pdf) {
            // `svg2pdf` does not number the fonts reproducibly
            // as soon as the PDF carries text (#162): two exports of the same
            // SVG differ by identifier bytes, not by content.
            assert_eq!(isolated.len(), bytes.len(), "Pdf: different length");
        } else {
            assert_eq!(
                &isolated, bytes,
                "{format:?} differs from the isolated export"
            );
        }
    }
    let empty = export_bytes_many(&plot, (480, 360), &measurer, &choice, &[]).expect("drawing");
    assert!(empty.is_empty());
}

/// Issues #77 and #78: the font and size set on a text zone
/// change the export — the axis title composed in Fira Sans 28 is not the one
/// composed in the graph's font, and the other zones do not change font.
#[test]
fn a_text_zones_font_changes_the_export() {
    let project = Project::new("Font per zone").expect("project");
    let mut spec = spec_with_label("Absorbance");
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let export = |plot: &stokesforge_core::resolve::ResolvedPlot<'_>| -> String {
        render_to_string_with_typst(
            plot,
            (500, 400),
            &measurer,
            &choice,
            stokesforge_render_svg::SvgTextMode::Outlines,
        )
        .expect("export")
    };
    let default = export(&plot);

    let font = stokesforge_plot::TextFont {
        family: "Fira Sans".to_owned(),
        size: Some(28.0),
        ..Default::default()
    };
    spec.title_font = font.clone();
    spec.title = "Title".to_owned();
    let mut panels = spec.panels.iter().cloned().collect::<Vec<_>>();
    panels[0].left.title_font = font;
    let spec = spec.with_panels(Panels::new(panels));
    let plot = resolve_spec(project.tree(), &spec);
    let set = export(&plot);
    assert_ne!(default, set, "the font and size changed nothing");
    if let Ok(path) = std::env::var("STOKESFORGE_TEST_SVG_OUT") {
        std::fs::write(path, &set).expect("writing the export");
    }
}

// ── #162: Typst text stays SVG text ──────────────────────────────────────

/// A graph with a title, axis titles, legend, formula and fraction.
fn rich_spec() -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.left.label = "$lambda_\"max\"$ (nm)".to_owned();
    panel.bottom.label = "Time $a/b$".to_owned();
    let mut spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    spec.title = "Spectrum alpha".to_owned();
    spec
}

fn svg_mode(spec: &PlotSpec, mode: stokesforge_render_svg::SvgTextMode) -> String {
    let project = Project::new("Text").expect("project");
    let plot = resolve_spec(project.tree(), spec);
    render_to_string_with_typst(
        &plot,
        (480, 360),
        &TextMeasurer::default(),
        &FontChoice::default(),
        mode,
    )
    .expect("export")
}

/// The contents of a document's `<text>` elements, concatenated.
fn texts(svg: &str) -> String {
    let mut out = String::new();
    for (start, _) in svg.match_indices("<text") {
        let Some(open_end) = svg[start..].find('>') else {
            continue;
        };
        let end = svg[start..]
            .find("</text>")
            .map_or(svg.len(), |i| start + i);
        out.push_str(&svg[start + open_end + 1..end]);
        out.push('\n');
    }
    out
}

#[test]
fn typst_text_comes_out_as_svg_text_and_the_geometry_does_not_move() {
    use stokesforge_render_svg::SvgTextMode::{Outlines, Text};
    let spec = rich_spec();
    let text = svg_mode(&spec, Text);
    let outlines = svg_mode(&spec, Outlines);

    let content = texts(&text);
    for expected in ["Spectrum", "alpha", "Time", "nm"] {
        assert!(
            content.contains(expected) || content.contains(&expected.chars().collect::<String>()),
            "{expected:?} missing from the <text>s:\n{content}"
        );
    }
    // The `lambda` formula comes out as the Unicode character (or, failing
    // that, as an outline), never as the `lambda` source.
    assert!(!content.contains("lambda"), "raw Typst source: {content}");
    // A named family with a generic fallback, never an `@font-face`.
    assert!(text.contains("font-family=\"'Noto Sans', sans-serif\""));
    assert!(!text.contains("@font-face"));

    // Outlines mode writes no text, ticks included.
    assert!(
        !outlines.contains("<text"),
        "a <text> remains in outlines mode:\n{outlines}"
    );
    // Same window, same frame: the figure does not move from one mode to the other.
    let frame = |svg: &str| svg.lines().next().map(str::to_owned);
    assert_eq!(frame(&text), frame(&outlines));
}

/// The ticks carry the graph's family, like the Typst texts.
#[test]
fn the_ticks_carry_the_graphs_font() {
    use stokesforge_render::{FontChoice, FontSet};
    use stokesforge_render_svg::SvgTextMode::Text;
    let project = Project::new("Font").expect("project");
    let spec = rich_spec();
    let plot = resolve_spec(project.tree(), &spec);
    for set in FontSet::ALL {
        let svg = render_to_string_with_typst(
            &plot,
            (480, 360),
            &TextMeasurer::with_font_choice(1.0, &FontChoice::Embedded(set)),
            &FontChoice::Embedded(set),
            Text,
        )
        .expect("export");
        assert!(
            !svg.contains("font-family=\"sans-serif\""),
            "generic tick with {set:?}"
        );
        // Plotters escapes the attribute's apostrophes.
        let family = format!(
            "font-family=\"&apos;{}&apos;, sans-serif\"",
            set.sans_family_name()
        );
        assert!(svg.contains(&family), "{family} missing with {set:?}");
    }
}

/// A glyph with no Unicode character (stretched delimiter of a large
/// fraction) goes back to an outline: a `<path>` with a glyph transformation.
#[test]
fn a_stretched_delimiter_goes_back_to_an_outline() {
    use stokesforge_render_svg::SvgTextMode::Text;
    let mut spec = rich_spec();
    spec.title = "$ lr(( a/b )) $".to_owned();
    let svg = svg_mode(&spec, Text);
    assert!(
        svg.contains("<path transform=\"translate("),
        "no outline glyph for a stretched delimiter:\n{svg}"
    );
}

/// The PDF derived from text mode contains real, extractable text.
#[test]
fn the_pdf_contains_extractable_text() {
    use stokesforge_render_svg::{export_bytes, ExportFormat};
    let project = Project::new("PDF").expect("project");
    let spec = rich_spec();
    let plot = resolve_spec(project.tree(), &spec);
    let pdf = export_bytes(
        &plot,
        (480, 360),
        &TextMeasurer::default(),
        &FontChoice::default(),
        ExportFormat::Pdf,
    )
    .expect("PDF");
    let document = lopdf::Document::load_mem(&pdf).expect("readable PDF");
    let page = *document.get_pages().keys().next().expect("one page");
    let extracted: String = document
        .extract_text(&[page])
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    assert!(
        extracted.contains("Time") && extracted.contains("Spectrum"),
        "PDF text not extractable: {extracted:?}"
    );
}
