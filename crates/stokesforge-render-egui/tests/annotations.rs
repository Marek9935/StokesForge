//! Free texts (issue #35): anchored to the data, they follow the zoom, and
//! a text outside the visible framing is not drawn.

use egui::{Pos2, Rect};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{AnnotationSpec, AxisScale, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::formula::FormulaCache;
use stokesforge_render::{FontChoice, PanelTransform, TextMeasurer};
use stokesforge_render_egui::{collect_shapes_with_typst, DrawnPlot, ScreenText, TypstScreen};

fn project_with_text() -> (Project, PanelSpec) {
    let column = |values: Vec<f64>| {
        Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values),
            ColumnMeta::default(),
        )
        .expect("column")
    };
    let mut project = Project::new("Annotations").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let xs: Vec<f64> = (1..=10).map(f64::from).collect();
    let x = project
        .add_column(sheet, None, Some("X"), column(xs.clone()))
        .expect("X");
    let y = project
        .add_column(sheet, None, Some("Y"), column(xs))
        .expect("Y");
    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(SeriesSpec::new_xy(x, y));
    panel
        .annotations
        .push(AnnotationSpec::new("Peak $lambda$", 5.0, 5.0));
    (project, panel)
}

fn plot_of(project: &Project, panel: PanelSpec) -> DrawnPlot {
    let ctx = egui::Context::default();
    ctx.set_fonts(stokesforge_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    let plot = resolve_spec(project.tree(), &spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(480.0, 360.0));
    let mut shapes = Vec::new();
    let mut drawn = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text = ScreenText::new(&ctx, &measurer);
        let typst = TypstScreen::new(&ctx, &cache, &choice).expect("fonts");
        drawn =
            Some(collect_shapes_with_typst(&mut shapes, rect, &text, typst, &plot).expect("plot"));
    });
    output.textures_delta.clear();
    drawn.expect("figure")
}

/// The drawn center is the projection of the data point, by the same
/// transform as the series — before and after a zoom.
#[test]
fn a_text_follows_the_zoom() {
    let (project, panel) = project_with_text();
    let center = |panel: PanelSpec| {
        let drawn = plot_of(&project, panel);
        let drawn_panel = &drawn.panels[0];
        let annotation = drawn_panel.annotations.first().expect("drawn text");
        let expected = PanelTransform::of(drawn_panel)
            .and_then(|t| t.data_to_pixel(5.0, 5.0))
            .expect("projection");
        // Within 1.5 px: the text is projected by Plotters, like the
        // series points, and `PanelTransform` (the picking) rounds
        // the edges of the area to within a pixel.
        assert!(
            (annotation.center.0 - expected.0).abs() <= 1.5
                && (annotation.center.1 - expected.1).abs() <= 1.5,
            "{:?} instead of {expected:?}",
            annotation.center
        );
        assert!(annotation.size.0 > 0 && annotation.size.1 > 0);
        annotation.center
    };
    let before = center(panel.clone());
    // What `ViewState::apply` sets for a zoom on 4..6.
    let mut zoomed = panel;
    zoomed.bottom.auto_range = false;
    zoomed.bottom.range_min = Some(4.0);
    zoomed.bottom.range_max = Some(6.0);
    let after = center(zoomed);
    assert!(
        (before.0 - after.0).abs() > 10.0,
        "the text did not follow the zoom"
    );
}

/// Outside the framing (zoom elsewhere), a free text stays drawn — it can
/// leave the plot area, into the margins (#163) —, but at a value
/// ≤ 0 on a log axis the scale does not represent it: not drawn.
#[test]
fn a_text_outside_the_framing_stays_drawn_but_not_off_scale() {
    let (project, panel) = project_with_text();
    let mut elsewhere = panel.clone();
    elsewhere.bottom.auto_range = false;
    elsewhere.bottom.range_min = Some(7.0);
    elsewhere.bottom.range_max = Some(9.0);
    assert_eq!(plot_of(&project, elsewhere).panels[0].annotations.len(), 1);

    let mut log = panel;
    log.left.scale = AxisScale::Log;
    log.annotations[0].y = -1.0;
    assert!(plot_of(&project, log).panels[0].annotations.is_empty());
}

/// #163: a text placed in the margins is **drawn** where it is
/// picked. Plotters clips each point to the rectangle of its area
/// (`BackendCoordOnly`): drawn on the plot area, the text stayed stuck
/// to its edge while its selection box went outside. The last
/// `Shape` is the text (drawn last); its center must be the picked
/// center.
#[test]
fn a_text_in_the_margins_is_drawn_where_it_is_picked() {
    let (project, panel) = project_with_text();
    for y in [0.0, -1.0] {
        let mut p = panel.clone();
        p.annotations[0].text = "test".into();
        p.annotations[0].y = y;
        let ctx = egui::Context::default();
        ctx.set_fonts(stokesforge_render::font_definitions());
        let measurer = TextMeasurer::default();
        let choice = FontChoice::default();
        let cache: FormulaCache<_> = FormulaCache::new();
        let spec = PlotSpec::empty("").with_panels(Panels::new(vec![p]));
        let plot = resolve_spec(project.tree(), &spec);
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(480.0, 360.0));
        let mut shapes = Vec::new();
        let mut drawn = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
            let text = ScreenText::new(&ctx, &measurer);
            let typst = TypstScreen::new(&ctx, &cache, &choice).expect("fonts");
            drawn = Some(
                collect_shapes_with_typst(&mut shapes, rect, &text, typst, &plot).expect("plot"),
            );
        });
        output.textures_delta.clear();
        let center = drawn.expect("figure").panels[0].annotations[0].center;
        let bounds = shapes.last().expect("a shape").visual_bounding_rect();
        assert!(
            (bounds.center().y as f64 - center.1).abs() <= 2.0,
            "y={y}: text drawn at {:?}, picked at {center:?}",
            bounds.center()
        );
    }
}

/// Fixed width (#244): the drawn box is the requested fraction of the
/// figure (480 px here), the long text wraps (taller box),
/// and without a width the text stays on one line.
#[test]
fn a_fixed_width_wraps_the_text() {
    let (project, mut panel) = project_with_text();
    let long = "A text far too long to fit on a single line in the box";
    panel.annotations[0].text = long.to_owned();
    let free = plot_of(&project, panel.clone()).panels[0].annotations[0];
    panel.annotations[0].width = Some(0.25);
    let boxed = plot_of(&project, panel.clone()).panels[0].annotations[0];
    assert!(
        (i64::from(boxed.size.0) - 120).abs() <= 2,
        "width {:?}",
        boxed.size
    );
    assert!(boxed.size.1 > free.size.1, "{boxed:?} against {free:?}");
    // The cap is the figure.
    panel.annotations[0].width = Some(7.0);
    let capped = plot_of(&project, panel).panels[0].annotations[0];
    assert!(capped.size.0 <= 482, "{capped:?}");
}

/// Width of a title area (#244): when fixed, it wraps a long axis title
/// — drawn text narrower and taller.
#[test]
fn the_width_of_an_axis_title_wraps_it() {
    use stokesforge_plot::{AxisSide, TextFont, TextTarget};
    let (project, mut panel) = project_with_text();
    panel.bottom.label = "An axis title really far too long for a single line".to_owned();
    let target = TextTarget::AxisTitle {
        panel: 0,
        side: AxisSide::Bottom,
    };
    let size_of = |panel: PanelSpec| {
        plot_of(&project, panel).panels[0]
            .texts
            .iter()
            .find(|t| t.target == target)
            .expect("drawn title")
            .size
    };
    let free = size_of(panel.clone());
    panel.bottom.title_font = TextFont {
        width: Some(0.2),
        ..TextFont::default()
    };
    let boxed = size_of(panel);
    assert!(boxed.0 < free.0 && boxed.1 > free.1, "{boxed:?} / {free:?}");
}
