//! The Typst screen path — task R8.3, and **the test of invariant 8.5**.
//!
//! Compiled permanently since issue #29 (no more `typst` feature):
//! `cargo test --workspace` now covers this path.

use egui::{FontFamily, FontId, Pos2, Rect, Shape};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{PanelSpec, Panels, PlotSpec};
use stokesforge_render::formula;
use stokesforge_render::formula::FormulaCache;
use stokesforge_render::text::{font_size, round_size, TextSource};
use stokesforge_render::{FontChoice, TextMeasurer};
use stokesforge_render_egui::{collect_shapes_with_typst, ScreenText, TypstScreen};

fn spec_with_label(label: &str) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.left.label = label.to_owned();
    panel.bottom.label = "t (s)".to_owned();
    PlotSpec::empty("").with_panels(Panels::new(vec![panel]))
}

/// **The most important test of the phase (mission instruction)**:
/// `estimate_text_size` must query Typst for a Typst label, not
/// `epaint::Fonts` — otherwise Plotters would reserve the margin of a
/// nine-character text `"$lambda$ (nm)"` rather than that of a rendered
/// formula, and the figure would shift without any error signaling it
/// (`architecture-choisie.md` §4.3).
///
/// Checked by comparing, on the same figure, the rendering produced with and
/// without the Typst path enabled on the backend: the drawn geometry (thus the
/// produced shapes) must differ, and the formula must appear as a textured
/// `Shape::Mesh` rather than as the `Shape::Text` of its literal source
/// string.
#[test]
fn estimate_text_size_queries_typst_for_a_formula_label() {
    let ctx = egui::Context::default();
    // Like `stokesforge-app` and the rest of the tests of this crate
    // (`cache_session.rs`): the single font source is set
    // explicitly.
    ctx.set_fonts(stokesforge_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();

    let project = Project::new("Typst").expect("project");
    let spec = spec_with_label("$lambda$ (nm)");
    let plot = resolve_spec(project.tree(), &spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));

    // Without Typst: the `epaint` measurement of the source string as is.
    // `load_texture`/`fonts_mut` only exist inside a frame
    // (`Context::run_ui`) — otherwise `epaint` panics (« No fonts
    // available until first call to Context::run() »), as
    // `cache_session.rs` already documents for this same pitfall.
    let mut shapes_without_typst = Vec::new();
    let mut panels_without_typst = 0;
    let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text_source = ScreenText::new(&ctx, &measurer);
        let drawn = stokesforge_render_egui::collect_shapes(
            &mut shapes_without_typst,
            rect,
            &text_source,
            &plot,
        )
        .expect("drawing without Typst failed");
        panels_without_typst = drawn.panels.len();
    });
    output.textures_delta.clear();

    // With Typst enabled on the backend.
    let mut shapes_with_typst = Vec::new();
    let mut panels_with_typst = 0;
    let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text_source = ScreenText::new(&ctx, &measurer);
        let typst = TypstScreen::new(&ctx, &cache, &choice).expect("font resolution failed");
        let drawn =
            collect_shapes_with_typst(&mut shapes_with_typst, rect, &text_source, typst, &plot)
                .expect("drawing with Typst failed");
        panels_with_typst = drawn.panels.len();
    });
    output.textures_delta.clear();

    // The figure was indeed drawn (no silent degradation to
    // "nothing": at least one panel, with its tick area), on both
    // sides.
    assert_eq!(panels_without_typst, 1);
    assert_eq!(panels_with_typst, 1);

    // The textured mesh of the formula is indeed present among the produced
    // shapes — the direct proof that the Typst path was taken,
    // not just that the measurement changed.
    let contains_a_mesh = shapes_with_typst
        .iter()
        .any(|shape| matches!(shape, Shape::Mesh(_)));
    assert!(
        contains_a_mesh,
        "no Shape::Mesh found: the Typst label did not take the expected rendering path"
    );

    // The two figures are **not** identical: the presence of the Typst path
    // changes the drawn geometry (width of the left tick area,
    // measured by Typst rather than by `epaint`), so at least one
    // shape differs between the two renderings.
    assert!(
        !shapes_without_typst.is_empty(),
        "the reference rendering is empty"
    );
    assert_ne!(
        format!("{shapes_without_typst:?}"),
        format!("{shapes_with_typst:?}"),
        "the rendering does not change whether Typst is enabled or not for this label"
    );
}

/// The `stokesforge_render::formula` measurement, isolated from any backend: a
/// formula does not measure the same as its literal source text seen by
/// `epaint` — a necessary condition for the test above to tell a
/// real Typst wiring from a bug that would always measure the source
/// string.
#[test]
fn the_typst_measurement_differs_from_the_epaint_measurement_of_the_literal_source_text() {
    let (sans_font, math) = formula::resolve_fonts(&FontChoice::default()).unwrap();
    let compiled =
        formula::compile("$lambda$ (nm)", 14.0, [0, 0, 0, 255], sans_font, math).unwrap();
    let typst_measurement = formula::measure(&compiled);

    let measurer = TextMeasurer::default();
    let font_id = FontId::new(font_size(14.0), FontFamily::Proportional);
    let epaint_measurement_of_source = round_size(measurer.measure("$lambda$ (nm)", font_id));

    assert_ne!(
        typst_measurement, epaint_measurement_of_source,
        "the formula and its raw source text measure the same: the \
         test would not tell a real Typst wiring from a bug that \
         would always measure the source string"
    );
}

/// The almost opaque pixels (alpha ≥ 200: at 1 pixel per point, a thin
/// stroke does not always reach 255) of the texture of a formula, read from the
/// texture delta of the frame that loaded it. **Premultiplied** colors,
/// as `ColorImage` keeps them.
fn formula_pixels(background: &str) -> Vec<egui::Color32> {
    let ctx = egui::Context::default();
    ctx.set_fonts(stokesforge_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();
    let project = Project::new("Typst").expect("project");
    let spec = PlotSpec {
        background: background.to_owned(),
        ..PlotSpec::empty("$Delta H$")
    }
    .with_panels(Panels::new(vec![PanelSpec::at(0, 0)]));
    let plot = resolve_spec(project.tree(), &spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));

    let mut shapes = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text_source = ScreenText::new(&ctx, &measurer);
        let typst = TypstScreen::new(&ctx, &cache, &choice).expect("fonts");
        collect_shapes_with_typst(&mut shapes, rect, &text_source, typst, &plot).expect("drawing");
    });
    let id = shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Mesh(mesh) => Some(mesh.texture_id),
            _ => None,
        })
        .expect("the Typst title produced no mesh");
    let (_, deltas) = output
        .textures_delta
        .set
        .iter()
        .find(|(texture, _)| **texture == id)
        .expect("title texture missing from the delta");
    let egui::ImageData::Color(image) = &deltas[0].image;
    let opaque = image
        .pixels
        .iter()
        .copied()
        .filter(|pixel| pixel.a() >= 200)
        .collect();
    output.textures_delta.clear();
    opaque
}

/// Issue #30: the texture of a Typst title carries the text color. On
/// a dark background, the automatic ink is white; the witness, on a light
/// background, stays dark. Before, both were black.
#[test]
fn the_texture_of_a_formula_takes_the_text_color() {
    let dark = formula_pixels("#101820");
    assert!(!dark.is_empty(), "no opaque pixel");
    // Premultiplied white: each component equals the opacity.
    assert!(
        dark.iter()
            .all(|pixel| pixel.r().abs_diff(pixel.a()) <= 3 && pixel.b().abs_diff(pixel.a()) <= 3),
        "Typst title not white on a dark background"
    );
    let light = formula_pixels("#ffffff");
    assert!(!light.is_empty(), "no opaque pixel (witness)");
    assert!(
        light.iter().all(|pixel| pixel.r() < 64),
        "the witness on a light background is not dark"
    );
}

// -- Issue #32: every title, axis title and legend goes through Typst --

/// A project with two named series, enough to fill a legend.
fn project_with_legend(names: [&str; 2]) -> (Project, PlotSpec) {
    project_with_columns(["Y0", "Y1"], names)
}

/// Like [`project_with_legend`], also naming the Y columns; an empty series
/// name lets the legend take that of the column.
fn project_with_columns(columns: [&str; 2], names: [&str; 2]) -> (Project, PlotSpec) {
    use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
    use stokesforge_plot::{LineSpec, SeriesSpec};
    let column = |values: Vec<f64>| {
        Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values),
            ColumnMeta::default(),
        )
        .expect("column")
    };
    let mut project = Project::new("Typst").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let xs: Vec<f64> = (0..20).map(f64::from).collect();
    let x = project
        .add_column(sheet, None, Some("X"), column(xs.clone()))
        .expect("X");
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom.label = "$lambda$".to_owned();
    panel.bottom.unit = "nm".to_owned();
    panel.left.label = "Absorbance".to_owned();
    for (index, (column_y, name)) in columns.into_iter().zip(names).enumerate() {
        let ys: Vec<f64> = xs.iter().map(|x| x * (index + 1) as f64).collect();
        let y = project
            .add_column(sheet, None, Some(column_y), column(ys))
            .expect("Y");
        let mut series = SeriesSpec::new_xy(x, y);
        series.line = LineSpec::from_palette(index);
        series.name = name.to_owned();
        panel.series.push(series);
    }
    let spec = PlotSpec::empty("Spectrum").with_panels(Panels::new(vec![panel]));
    (project, spec)
}

/// Draws `spec` on screen, with Typst enabled or not.
fn screen(
    project: &Project,
    spec: &PlotSpec,
    with_typst: bool,
) -> (Vec<Shape>, stokesforge_render_egui::DrawnPlot) {
    let ctx = egui::Context::default();
    ctx.set_fonts(stokesforge_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();
    let plot = resolve_spec(project.tree(), spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(480.0, 360.0));
    let mut shapes = Vec::new();
    let mut drawn = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text = ScreenText::new(&ctx, &measurer);
        drawn = Some(if with_typst {
            let typst = TypstScreen::new(&ctx, &cache, &choice).expect("fonts");
            collect_shapes_with_typst(&mut shapes, rect, &text, typst, &plot).expect("drawing")
        } else {
            stokesforge_render_egui::collect_shapes(&mut shapes, rect, &text, &plot)
                .expect("drawing")
        });
    });
    output.textures_delta.clear();
    (shapes, drawn.expect("figure"))
}

/// The unrotated top-left corner of each Typst mesh — its first
/// vertex (`typst_screen::mesh`) —, to the half pixel, sorted.
fn screen_origins(shapes: &[Shape]) -> Vec<(i64, i64)> {
    let mut origins: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => mesh.vertices.first().map(|v| v.pos),
            _ => None,
        })
        .map(|pos| ((pos.x * 2.0).round() as i64, (pos.y * 2.0).round() as i64))
        .collect();
    origins.sort_unstable();
    origins
}

/// The same reading in the exported SVG: the origin of each Typst group
/// (`translate(ox, oy)`, `typst_export::group_for`).
fn export_origins(document: &str) -> Vec<(i64, i64)> {
    let mut origins: Vec<_> = document
        .match_indices(") translate(")
        .filter_map(|(index, pattern)| {
            let rest = &document[index + pattern.len()..];
            let (x, rest) = rest.split_once(", ")?;
            let (y, _) = rest.split_once(')')?;
            let (x, y): (f64, f64) = (x.parse().ok()?, y.parse().ok()?);
            Some(((x * 2.0).round() as i64, (y * 2.0).round() as i64))
        })
        .collect();
    origins.sort_unstable();
    origins
}

/// A title **without `$`** is measured by Typst: the title band does not have the
/// same height as the `epaint` measurement of the same text (the witness), and the title
/// is no longer a `Shape::Text`. The ticks, for their part, stay `epaint`
/// text.
#[test]
fn a_title_without_dollar_is_measured_and_drawn_by_typst() {
    let (project, spec) = project_with_legend(["A", "B"]);
    let (with, typst_figure) = screen(&project, &spec, true);
    let (_, epaint_figure) = screen(&project, &spec, false);
    assert_ne!(
        typst_figure.title_height, epaint_figure.title_height,
        "the title « Spectrum » is still measured by epaint"
    );
    let texts: Vec<String> = with
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    assert!(!texts.is_empty(), "no epaint tick left");
    for text in texts {
        assert!(text.parse::<f64>().is_ok(), "{text:?} stayed epaint text");
    }
}

/// Screen and export place the same Typst boxes at the same pixels: title,
/// two axis titles and two legend entries, including texts without `$`.
/// The witness is the export without Typst, which places none.
#[test]
fn screen_and_export_place_the_typst_texts_at_the_same_place() {
    let (project, spec) = project_with_legend(["$\"Ru(bpy)\"_3^(2+)$", "Témoin"]);
    let (shapes, _) = screen(&project, &spec, true);
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let on_screen = screen_origins(&shapes);
    assert_eq!(on_screen.len(), 5, "title, two axis titles, two entries");
    // The two ways of writing the text (#162) place the passages at the same place.
    for mode in [
        stokesforge_render_svg::SvgTextMode::Outlines,
        stokesforge_render_svg::SvgTextMode::Text,
    ] {
        let document = stokesforge_render_svg::render_to_string_with_typst(
            &plot,
            (480, 360),
            &measurer,
            &FontChoice::default(),
            mode,
        )
        .expect("export");
        let export = export_origins(&document);
        match mode {
            // The ticks also go through Typst there: their origins
            // are added to those of the five screen texts.
            stokesforge_render_svg::SvgTextMode::Outlines => {
                for origin in &on_screen {
                    assert!(export.contains(origin), "{origin:?} missing: {export:?}");
                }
                assert!(export.len() > on_screen.len(), "ticks not outlined");
            }
            stokesforge_render_svg::SvgTextMode::Text => assert_eq!(on_screen, export),
        }
    }

    let without =
        stokesforge_render_svg::render_to_string(&plot, (480, 360), &measurer).expect("export");
    assert!(
        export_origins(&without).is_empty(),
        "the witness has Typst groups"
    );
}

/// A mixed legend — one formula entry, one without — has lines of the same
/// height: the two texts have the same box (the strut of
/// `formula::compile`), so the thumbnail, centered on the line, is also
/// centered on each of the two.
#[test]
fn a_mixed_legend_has_lines_of_the_same_height() {
    let (project, spec) = project_with_legend(["$\"Ru(bpy)\"_3^(2+)$", "Témoin"]);
    let (shapes, _) = screen(&project, &spec, true);
    let heights: Vec<f32> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => Some(mesh.calc_bounds()),
            _ => None,
        })
        // The two legend entries: at the top right of the plot area,
        // under the title and far from the bottom axis title.
        .filter(|bounds| bounds.min.x > 240.0 && (40.0..200.0).contains(&bounds.min.y))
        .map(|bounds| bounds.height())
        .collect();
    assert_eq!(heights.len(), 2, "two entries expected: {heights:?}");
    assert!(
        (heights[0] - heights[1]).abs() < 0.01,
        "different line heights: {heights:?}"
    );
}

/// The meshes of the legend entries (at the top right of the plot
/// area), in line order.
fn legend_entries(shapes: &[Shape]) -> Vec<egui::Rect> {
    let mut entries: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => Some(mesh.calc_bounds()),
            _ => None,
        })
        .filter(|bounds| bounds.min.x > 240.0 && (40.0..200.0).contains(&bounds.min.y))
        .collect();
    entries.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    entries
}

/// Issue #33: a column name taken over in the legend is composed by Typst
/// **as is**, whereas a typed name is composed as markup. The same string `$lambda$`
/// gives a formula (one letter) when typed, and eight characters when taken over from a
/// column. And the path without Typst shows the original name, never its escaped
/// form `#"…"`.
#[test]
fn a_column_name_is_shown_as_is_a_typed_name_is_composed() {
    let (project, spec) = project_with_columns(["Abs_280 #1", "$lambda$"], ["", "$lambda$"]);
    let (shapes, _) = screen(&project, &spec, true);
    let [column_entry, typed] = legend_entries(&shapes)[..] else {
        panic!("two Typst legend entries expected")
    };
    assert!(column_entry.width() > 0.0 && typed.width() > 0.0);

    let (project, taken_over) = project_with_columns(["Abs_280 #1", "$lambda$"], ["", ""]);
    let (shapes_taken_over, _) = screen(&project, &taken_over, true);
    let [_, literal] = legend_entries(&shapes_taken_over)[..] else {
        panic!("two Typst legend entries expected")
    };
    assert!(
        literal.width() > 2.0 * typed.width(),
        "the column name « $lambda$ » was read as a formula"
    );

    let (without_typst, _) = screen(&project, &taken_over, false);
    let texts: Vec<String> = without_typst
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|text| text == "Abs_280 #1"), "{texts:?}");
    assert!(!texts.iter().any(|text| text.starts_with("#\"")));
}

/// The red frames (closed paths) painted for the figure of `label` (left axis
/// title) with the Typst path active.
fn red_frames(label: &str) -> usize {
    let ctx = egui::Context::default();
    ctx.set_fonts(stokesforge_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();
    let project = Project::new("Typst").expect("project");
    let spec = spec_with_label(label);
    let plot = resolve_spec(project.tree(), &spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
    let mut shapes = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text = ScreenText::new(&ctx, &measurer);
        let typst = TypstScreen::new(&ctx, &cache, &choice).expect("fonts");
        collect_shapes_with_typst(&mut shapes, rect, &text, typst, &plot).expect("drawing");
    });
    output.textures_delta.clear();
    shapes
        .iter()
        .filter(|shape| match shape {
            Shape::Path(path) => {
                path.closed
                    && matches!(&path.stroke.color,
                        egui::epaint::ColorMode::Solid(c) if c.r() > 200 && c.g() < 80)
            }
            _ => false,
        })
        .count()
}

/// T3 (#240): a title that Typst refuses is drawn raw **in a red
/// frame** on screen; a valid title has none. (The exports go through the
/// SVG backend, which draws no frame.)
#[test]
fn a_text_that_typst_refuses_is_framed_in_red_on_screen() {
    assert_eq!(red_frames("$x, x_, *bold"), 1);
    assert_eq!(red_frames("$lambda$ (nm)"), 0);
}
