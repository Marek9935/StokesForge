//! `figure_layout` (#394): the single place where the frames (plot areas), the
//! axis bands, the title band and the page meet, and the cells and frames the
//! pipeline draws from it.

use plotters::drawing::IntoDrawingArea;
use plotters_backend::{
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
};
use stokesforge_core::column::Column;
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{OverlaySpec, PanelSpec, Panels, PlotSpec, SeriesAxis, SeriesSpec};
use stokesforge_render::pipeline::{self, FigureLayout};
use stokesforge_render::text::{estimate, TextMeasurer};
use stokesforge_render::DrawnPlot;

/// A backend that discards everything but keeps the project's measurement.
struct Mute {
    size: (u32, u32),
    measurer: TextMeasurer,
}

#[derive(Debug)]
enum Never {}

impl std::fmt::Display for Never {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for Never {}

impl DrawingBackend for Mute {
    type ErrorType = Never;
    fn get_size(&self) -> (u32, u32) {
        self.size
    }
    fn ensure_prepared(&mut self) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }
    fn present(&mut self) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }
    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        _path: I,
        _style: &S,
    ) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }
    fn draw_pixel(
        &mut self,
        _point: BackendCoord,
        _color: BackendColor,
    ) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }
    fn draw_text<S: BackendTextStyle>(
        &mut self,
        _text: &str,
        _style: &S,
        _pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Never>> {
        Ok(())
    }
    fn estimate_text_size<S: BackendTextStyle>(
        &self,
        text: &str,
        style: &S,
    ) -> Result<(u32, u32), DrawingErrorKind<Never>> {
        Ok(estimate(&self.measurer, text, style))
    }
}

fn mute(size: (u32, u32)) -> Mute {
    Mute {
        size,
        measurer: TextMeasurer::default(),
    }
}

/// A grid of empty layers, `rows` by `columns`, with the given title.
fn grid(title: &str, rows: usize, columns: usize) -> PlotSpec {
    let panels: Vec<PanelSpec> = (0..rows)
        .flat_map(|r| (0..columns).map(move |c| PanelSpec::at(r, c)))
        .collect();
    PlotSpec::empty(title).with_panels(Panels::new(panels))
}

fn layout_of(spec: &PlotSpec) -> FigureLayout {
    let probe = mute((1, 1)).into_drawing_area();
    pipeline::figure_layout(spec, &probe).expect("layout")
}

/// A project with one live column, to feed a series.
fn project_with_column() -> (Project, NodeId) {
    let mut project = Project::new("Layout").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let column = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("column");
    (project, column)
}

fn draw_in(project: &Project, spec: &PlotSpec, size: (u32, u32)) -> DrawnPlot {
    let plot = resolve_spec(project.tree(), spec);
    pipeline::draw(mute(size), &plot).expect("drawing")
}

fn draw_at(spec: &PlotSpec, size: (u32, u32)) -> DrawnPlot {
    draw_in(&Project::new("Layout").expect("project"), spec, size)
}

/// Draws the spec at the size its own layout gives.
fn drawn(spec: &PlotSpec) -> (FigureLayout, DrawnPlot) {
    let layout = layout_of(spec);
    let plot = draw_at(spec, layout.size);
    (layout, plot)
}

/// The cells actually drawn, `(origin, size)`, by layer index.
fn cells(drawn: &DrawnPlot) -> Vec<((i32, i32), (u32, u32))> {
    drawn
        .panels
        .iter()
        .map(|p| (p.cell_origin, p.cell_size))
        .collect()
}

/// The frames actually drawn, `(origin, size)`, by layer index.
fn frames(drawn: &DrawnPlot) -> Vec<((i32, i32), (u32, u32))> {
    drawn
        .panels
        .iter()
        .map(|p| (p.pixel_origin, p.pixel_size))
        .collect()
}

fn framed(mut spec: PlotSpec, widths: &[f64], heights: &[f64]) -> PlotSpec {
    spec.frame_widths = widths.to_vec();
    spec.frame_heights = heights.to_vec();
    spec
}

#[test]
fn a_legacy_graph_is_480_by_360_shared_equally_title_band_inside() {
    let spec = grid("Title", 1, 2);
    let layout = layout_of(&spec);
    assert_eq!(layout.size, (480, 360));
    assert!(!layout.is_explicit());
    assert!(layout.title_band > 0, "the title takes a band");
    assert_eq!(layout.column_breaks, vec![0, 240, 480]);
    assert_eq!(layout.row_breaks, vec![0, 360 - layout.title_band]);
    let drawn = draw_at(&spec, layout.size);
    let band = layout.title_band as i32;
    assert_eq!(
        cells(&drawn),
        vec![
            ((0, band), (240, 360 - band as u32)),
            ((240, band), (240, 360 - band as u32))
        ]
    );
}

/// The frame is what is stored: axes are added around it, the image grows.
#[test]
fn the_frame_is_the_stored_size_and_the_bands_come_on_top() {
    let spec = framed(grid("", 1, 1), &[300.0], &[200.0]);
    let (layout, plot) = drawn(&spec);
    assert!(layout.is_explicit());
    assert_eq!(frames(&plot), vec![((12 + 64, 12), (300, 200))]);
    // Margins (2 x 12) and the left axis (64) beside; margins and the bottom
    // axis (52) below.
    assert_eq!(layout.size, (300 + 24 + 64, 200 + 24 + 52));
    assert_eq!(layout.frame_rect(0, 0), Some((76, 12, 300, 200)));
}

#[test]
fn adding_a_right_axis_widens_the_image_and_never_moves_or_resizes_the_frame() {
    let (project, column) = project_with_column();
    let bare = framed(grid("", 1, 1), &[300.0], &[200.0]);
    let (bare_layout, bare_plot) = drawn(&bare);
    let mut spec = bare.clone();
    let mut series = SeriesSpec::new(column);
    series.axis = SeriesAxis::Right;
    {
        let panel = spec.panels.get_mut(0).expect("panel");
        panel.right.visible = true;
        panel.series.push(series);
    }
    let layout = layout_of(&spec);
    let plot = draw_in(&project, &spec, layout.size);
    assert_eq!(frames(&plot), frames(&bare_plot), "same frame, same place");
    assert_eq!(layout.size.0, bare_layout.size.0 + 64);
    assert_eq!(layout.size.1, bare_layout.size.1);
}

#[test]
fn adding_the_graph_title_grows_the_image_by_its_band_and_lowers_the_frame() {
    let bare = framed(grid("", 1, 1), &[300.0], &[200.0]);
    let (bare_layout, bare_plot) = drawn(&bare);
    let mut spec = bare.clone();
    spec.title = "A title".into();
    let (layout, plot) = drawn(&spec);
    assert!(layout.title_band > 0);
    assert_eq!(layout.size.0, bare_layout.size.0);
    assert_eq!(layout.size.1, bare_layout.size.1 + layout.title_band);
    let (bare_frame, frame) = (frames(&bare_plot)[0], frames(&plot)[0]);
    assert_eq!(frame.1, bare_frame.1, "same size");
    assert_eq!(frame.0 .0, bare_frame.0 .0);
    assert_eq!(frame.0 .1, bare_frame.0 .1 + layout.title_band as i32);
}

#[test]
fn a_top_overlay_axis_adds_one_band_per_level_above_the_unchanged_frame() {
    let bare = framed(grid("", 1, 1), &[300.0], &[200.0]);
    let (bare_layout, bare_plot) = drawn(&bare);
    let mut spec = bare.clone();
    let base = spec.panels.first().expect("panel").clone();
    let mut panels = vec![base.clone()];
    panels.push(PanelSpec::overlay_on(
        &base,
        OverlaySpec {
            x_top: true,
            y_right: false,
            layer: 2,
        },
    ));
    spec = spec.with_panels(Panels::new(panels));
    let (layout, plot) = drawn(&spec);
    assert_eq!(layout.size.1, bare_layout.size.1 + 52);
    assert_eq!(
        frames(&plot)[0].1,
        frames(&bare_plot)[0].1,
        "the frame keeps its size"
    );
}

/// Two rows, a right axis only in the first: the frames of the column line up.
#[test]
fn the_frames_of_a_column_share_their_edges_whatever_each_cell_asks_for() {
    let (project, column) = project_with_column();
    let mut spec = framed(grid("", 2, 1), &[300.0], &[150.0, 150.0]);
    let mut series = SeriesSpec::new(column);
    series.axis = SeriesAxis::Right;
    {
        let panel = spec.panels.get_mut(0).expect("panel");
        panel.right.visible = true;
        panel.series.push(series);
    }
    let layout = layout_of(&spec);
    let plot = draw_in(&project, &spec, layout.size);
    let f = frames(&plot);
    assert_eq!(f.len(), 2);
    assert_eq!(f[0].0 .0, f[1].0 .0, "same left edge");
    assert_eq!(f[0].1 .0, f[1].1 .0, "same width");
    assert_eq!((f[0].1 .0, f[1].1 .0), (300, 300));
    assert_eq!(layout.size.0, 300 + 24 + 64 + 64);
}

/// A title above one cell of a row lowers the frames of the whole row.
#[test]
fn the_frames_of_a_row_share_their_edges_whatever_each_title_asks_for() {
    let mut spec = framed(grid("", 1, 2), &[200.0, 200.0], &[150.0]);
    spec.panels.get_mut(0).expect("panel").title = "Left".into();
    let (layout, plot) = drawn(&spec);
    let f = frames(&plot);
    assert_eq!(f[0].0 .1, f[1].0 .1, "same top edge");
    assert_eq!(f[0].1 .1, f[1].1 .1, "same height");
    assert_eq!(f[0].1 .1, 150);
    assert!(layout.row_gutters[0].0 > 12, "the title band is in the row");
}

#[test]
fn a_cell_added_beside_another_has_the_same_frame() {
    let spec = framed(grid("", 1, 2), &[300.0, 300.0], &[200.0]);
    let (_, plot) = drawn(&spec);
    let f = frames(&plot);
    assert_eq!(f[0].1, f[1].1);
    assert_eq!(f[0].1, (300, 200));
}

#[test]
fn breakpoints_are_rounded_prefix_sums_never_rounded_widths() {
    let spec = framed(grid("", 1, 3), &[100.4, 100.4, 100.4], &[100.0]);
    let layout = layout_of(&spec);
    // Each column is 100.4 + 12 + 12 + 64 (left axis) = 188.4 wide; with the
    // bottom axis band (52) each row 100 + 24 + 52.
    assert_eq!(layout.size.0, 565);
    assert_eq!(layout.column_breaks, vec![0, 188, 377, 565]);
    let plot = draw_at(&spec, layout.size);
    let widths: Vec<u32> = cells(&plot).iter().map(|(_, s)| s.0).collect();
    assert_eq!(widths.iter().sum::<u32>(), 565);
}

#[test]
fn a_grid_beyond_the_cell_cap_reads_the_legacy_split() {
    let spec = framed(
        PlotSpec::empty("").with_panels(Panels::new(vec![
            PanelSpec::at(0, 0),
            PanelSpec::at(usize::MAX, 0),
        ])),
        &[1000.0],
        &[1000.0],
    );
    assert_eq!(layout_of(&spec).size, (480, 360));
}

#[test]
fn hostile_frames_draw_without_panicking() {
    let mut spec = framed(grid("T", 2, 2), &[], &[f64::INFINITY]);
    spec.frame_widths = vec![f64::NAN, 1e300, -3.0, 5.0, 7.0, 9.0];
    let (layout, plot) = drawn(&spec);
    assert!(layout.size.0 <= 40_000 && layout.size.1 <= 40_000);
    assert_eq!(plot.omitted_panels, 0, "a stored frame is never omitted");
}

#[test]
fn the_cell_is_the_outer_boundary_of_the_frame_and_its_bands() {
    let spec = framed(grid("T", 1, 2), &[300.0, 200.0], &[150.0]);
    let (layout, plot) = drawn(&spec);
    let band = layout.title_band as i32;
    let c = cells(&plot);
    assert_eq!(c[0], ((0, band), (300 + 24 + 64, 150 + 24 + 52)));
    assert_eq!(c[1].0, (300 + 24 + 64, band));
    assert_eq!(layout.cell_rect(0, 1).map(|r| (r.2, r.3)), Some(c[1].1));
}

/// What `legacy_frames` promises: the plot areas a legacy graph is drawn with.
fn legacy_frames_of(spec: &PlotSpec) -> Option<(Vec<f64>, Vec<f64>)> {
    let probe = mute((1, 1)).into_drawing_area();
    pipeline::legacy_frames(spec, &probe).expect("legacy frames")
}

#[test]
fn legacy_frames_are_the_plot_areas_the_legacy_layout_draws() {
    let mut titled = grid("Title", 1, 2);
    titled.panels.get_mut(1).expect("panel").title = "Panel".into();
    // A right axis in one cell only: the frames of its column differ.
    let (project, column) = project_with_column();
    let mut uneven = grid("", 2, 1);
    {
        let mut series = SeriesSpec::new(column);
        series.axis = SeriesAxis::Right;
        let panel = uneven.panels.get_mut(0).expect("panel");
        panel.right.visible = true;
        panel.series.push(series);
    }
    for spec in [
        grid("", 1, 1),
        grid("Title", 1, 1),
        grid("Title", 2, 2),
        titled,
        uneven,
    ] {
        let (widths, heights) = legacy_frames_of(&spec).expect("legacy");
        let plot = draw_in(&project, &spec, (480, 360));
        for (column, width) in widths.iter().enumerate() {
            let drawn = plot
                .panels
                .iter()
                .filter(|p| p.column == column)
                .map(|p| p.pixel_size.0)
                .max()
                .unwrap();
            assert_eq!(*width as u32, drawn, "column {column}");
        }
        for (row, height) in heights.iter().enumerate() {
            let drawn = plot
                .panels
                .iter()
                .filter(|p| p.row == row)
                .map(|p| p.pixel_size.1)
                .max()
                .unwrap();
            assert_eq!(*height as u32, drawn, "row {row}");
        }
    }
}

/// Fixing a legacy graph in place does not move anything: same plot area,
/// same cell boundaries.
#[test]
fn drawing_with_the_legacy_frames_gives_the_legacy_picture() {
    for spec in [grid("", 1, 1), grid("Title", 1, 1), grid("Title", 1, 2)] {
        let legacy_layout = layout_of(&spec);
        let legacy = draw_at(&spec, legacy_layout.size);
        let (widths, heights) = legacy_frames_of(&spec).expect("legacy");
        let mut fixed = spec.clone();
        fixed.frame_widths = widths;
        fixed.frame_heights = heights;
        let layout = layout_of(&fixed);
        let plot = draw_at(&fixed, layout.size);
        assert_eq!(frames(&plot), frames(&legacy));
        assert_eq!(layout.size, legacy_layout.size, "same figure");
    }
}

#[test]
fn a_graph_that_stores_its_frames_has_no_legacy_frames() {
    assert!(legacy_frames_of(&framed(grid("", 1, 1), &[300.0], &[200.0])).is_none());
}

/// A small frame is not squeezed by its bands: the cap of a third of the
/// cell, which protects a legacy cell, does not apply when the frame is fixed.
#[test]
fn a_small_frame_keeps_its_size_whatever_its_bands() {
    let (project, column) = project_with_column();
    let mut spec = framed(grid("", 1, 1), &[40.0], &[36.0]);
    let mut series = SeriesSpec::new(column);
    series.axis = SeriesAxis::Right;
    {
        let panel = spec.panels.get_mut(0).expect("panel");
        panel.right.visible = true;
        panel.series.push(series);
    }
    let layout = layout_of(&spec);
    let plot = draw_in(&project, &spec, layout.size);
    assert_eq!(frames(&plot)[0].1, (40, 36));
    assert_eq!(plot.omitted_panels, 0);
}

/// `size_with_frames` is the size a draft would give: the current frames give
/// the current size, a wider frame adds exactly its difference.
#[test]
fn size_with_frames_adds_the_difference_to_the_bands_of_the_figure() {
    let spec = framed(grid("T", 1, 2), &[200.0, 200.0], &[150.0]);
    let layout = layout_of(&spec);
    let (w, h) = layout.size_with_frames(&layout.frame_widths, &layout.frame_heights);
    assert_eq!((w.round() as u32, h.round() as u32), layout.size);
    let (wider, _) = layout.size_with_frames(&[300.0, 200.0], &layout.frame_heights);
    assert!((wider - w - 100.0).abs() < 1e-9);
}

// ---- "Figure size = Fixed", Left and Top (#405) ----

fn fixed(mut spec: PlotSpec) -> PlotSpec {
    spec.fixed_figure = true;
    spec
}

/// A layer superimposed on the first one, with a right axis and a top axis.
fn with_overlay(spec: PlotSpec, layer: u8) -> PlotSpec {
    let mut panels: Vec<PanelSpec> = spec.panels.iter().cloned().collect();
    let base = panels.first().expect("panel").clone();
    panels.push(PanelSpec::overlay_on(
        &base,
        OverlaySpec {
            x_top: true,
            y_right: true,
            layer,
        },
    ));
    spec.with_panels(Panels::new(panels))
}

/// The four bands of one axis per side are reserved: the image is the frame
/// plus 2 x 76 and 2 x 64.
#[test]
fn a_fixed_figure_reserves_one_axis_on_each_side() {
    let spec = fixed(framed(grid("", 1, 1), &[300.0], &[200.0]));
    let layout = layout_of(&spec);
    assert_eq!(layout.size, (300 + 152, 200 + 128));
    assert_eq!(layout.frame_rect(0, 0), Some((76, 64, 300, 200)));
    assert!(!layout.fixed_overflow);
}

/// Adding a right axis, a top mirror and an overlay changes neither the size
/// nor the frame: they fill the reserve.
#[test]
fn a_fixed_figure_keeps_its_size_and_frame_when_axes_come_in_the_reserve() {
    let bare = fixed(framed(grid("", 1, 1), &[300.0], &[200.0]));
    let bare_layout = layout_of(&bare);
    let mut spec = with_overlay(bare.clone(), 2);
    {
        let panel = spec.panels.get_mut(0).expect("panel");
        panel.right.visible = true;
        panel.top.visible = true;
    }
    let layout = layout_of(&spec);
    assert_eq!(layout.size, bare_layout.size);
    assert_eq!(layout.frame_rect(0, 0), bare_layout.frame_rect(0, 0));
    assert!(!layout.fixed_overflow);
    let plot = draw_at(&spec, layout.size);
    let (bare_plot, _) = (draw_at(&bare, bare_layout.size), 0);
    assert_eq!(frames(&plot)[0], frames(&bare_plot)[0]);
}

/// Without the option the same additions widen the image (the default of #394
/// is untouched).
#[test]
fn without_the_option_the_same_additions_still_grow_the_image() {
    let bare = framed(grid("", 1, 1), &[300.0], &[200.0]);
    let bare_layout = layout_of(&bare);
    let layout = layout_of(&with_overlay(bare, 2));
    assert!(layout.size.0 > bare_layout.size.0);
    assert!(layout.size.1 > bare_layout.size.1);
    assert!(!layout.fixed_overflow);
}

/// A legacy graph (nothing stored) ignores the option: still 480 x 360.
#[test]
fn a_legacy_graph_ignores_the_fixed_figure_and_the_position() {
    let mut spec = fixed(grid("Title", 1, 2));
    spec.figure_left = Some(150.0);
    spec.figure_top = Some(150.0);
    let layout = layout_of(&spec);
    assert_eq!(layout.size, (480, 360));
    assert!(!layout.is_explicit() && !layout.fixed_overflow);
    assert_eq!(layout_of(&grid("Title", 1, 2)), layout);
}

/// Two layers on the right ask for 128 > the 64 reserved: the image grows
/// anyway, the frame does not move, and the figure says so.
#[test]
fn a_fixed_figure_grows_past_the_reserve_without_moving_the_frame() {
    let bare = fixed(framed(grid("", 1, 1), &[300.0], &[200.0]));
    let bare_layout = layout_of(&bare);
    let one = with_overlay(bare.clone(), 2);
    let two = with_overlay(one.clone(), 3);
    assert!(!layout_of(&one).fixed_overflow);
    let layout = layout_of(&two);
    assert!(layout.fixed_overflow);
    // One more band on the right (64) and on top (52): the frame keeps its
    // size and its left edge, and sits one band lower in the taller image.
    assert_eq!(
        layout.size,
        (bare_layout.size.0 + 64, bare_layout.size.1 + 52)
    );
    let (bx, by, bw, bh) = bare_layout.frame_rect(0, 0).expect("frame");
    assert_eq!(layout.frame_rect(0, 0), Some((bx, by + 52, bw, bh)));
}

/// A title is not part of the reserve (F2): showing it grows the image, the
/// frame keeps its size, and the figure says so.
#[test]
fn a_title_grows_a_fixed_figure_and_is_reported() {
    let bare = fixed(framed(grid("", 1, 1), &[300.0], &[200.0]));
    let bare_layout = layout_of(&bare);
    let mut spec = bare.clone();
    spec.title = "A title".into();
    let layout = layout_of(&spec);
    assert!(layout.title_band > 0 && layout.fixed_overflow);
    assert_eq!(layout.size.1, bare_layout.size.1 + layout.title_band);
    assert_eq!(layout.size.0, bare_layout.size.0);
    let (x, y, w, h) = layout.frame_rect(0, 0).expect("frame");
    assert_eq!((x, w, h), (76, 300, 200));
    assert_eq!(y, 64 + layout.title_band);
}

/// A column of two cells: the frames still line up in Fixed mode.
#[test]
fn the_frames_of_a_fixed_column_still_line_up() {
    let spec = fixed(framed(grid("", 2, 1), &[300.0], &[150.0, 150.0]));
    let (layout, plot) = drawn(&spec);
    let f = frames(&plot);
    assert_eq!(f[0].0 .0, f[1].0 .0);
    assert_eq!(f[0].1, f[1].1.clone());
    assert_eq!(layout.frame_rect(0, 0).map(|r| r.0), Some(76));
}

/// Left and Top place the first frame, in Fixed mode and in Auto.
#[test]
fn left_and_top_place_the_first_frame_and_grow_the_image() {
    for fixed_mode in [false, true] {
        let mut auto = framed(grid("", 1, 1), &[300.0], &[200.0]);
        auto.fixed_figure = fixed_mode;
        let auto_layout = layout_of(&auto);
        let mut spec = auto.clone();
        spec.figure_left = Some(150.0);
        spec.figure_top = Some(110.0);
        let (layout, plot) = drawn(&spec);
        let (x, y, w, h) = layout.frame_rect(0, 0).expect("frame");
        assert_eq!((x, y, w, h), (150, 110, 300, 200), "fixed = {fixed_mode}");
        let auto_frame = auto_layout.frame_rect(0, 0).expect("frame");
        assert_eq!(
            layout.size.0 as i32 - auto_layout.size.0 as i32,
            150 - auto_frame.0 as i32
        );
        assert_eq!(
            layout.size.1 as i32 - auto_layout.size.1 as i32,
            110 - auto_frame.1 as i32
        );
        assert_eq!(frames(&plot)[0], ((150, 110), (300, 200)));
    }
}

/// Top is measured from the top of the image, graph title included.
#[test]
fn top_includes_the_graph_title_band() {
    let mut spec = fixed(framed(grid("A title", 1, 1), &[300.0], &[200.0]));
    spec.figure_top = Some(130.0);
    let (layout, plot) = drawn(&spec);
    assert!(layout.title_band > 0 && layout.title_band < 66);
    assert_eq!(layout.frame_rect(0, 0).map(|r| r.1), Some(130));
    assert_eq!(frames(&plot)[0].0 .1, 130, "the drawing agrees");
    assert!(!layout.fixed_overflow, "the title fits above the frame");
}

/// A value smaller than the axes need is raised to the minimum, which the
/// layout reports.
#[test]
fn a_small_left_or_top_is_raised_to_what_the_axes_and_title_need() {
    let mut spec = framed(grid("A title", 1, 1), &[300.0], &[200.0]);
    spec.figure_left = Some(5.0);
    spec.figure_top = Some(5.0);
    let layout = layout_of(&spec);
    assert_eq!(layout.min_left, 76);
    assert_eq!(layout.min_top, layout.title_band + 12);
    let (x, y, _, _) = layout.frame_rect(0, 0).expect("frame");
    assert_eq!((x, y), (76, layout.title_band + 12));
    spec.fixed_figure = true;
    let layout = layout_of(&spec);
    assert_eq!(layout.frame_rect(0, 0).map(|r| r.0), Some(76));
    assert!(
        layout.fixed_overflow,
        "Top = 5 is below what the title needs"
    );
    // Left alone: the left axis needs 76, so 5 grows the image.
    let mut left_only = fixed(framed(grid("", 1, 1), &[300.0], &[200.0]));
    left_only.figure_left = Some(5.0);
    assert!(layout_of(&left_only).fixed_overflow);
    left_only.figure_left = Some(80.0);
    assert!(!layout_of(&left_only).fixed_overflow);
}

/// Hostile values read as Auto or are clamped, and never panic.
#[test]
fn hostile_left_and_top_draw_without_panicking() {
    for value in [f64::NAN, f64::INFINITY, -4.0, 1e12] {
        let mut spec = fixed(framed(grid("T", 2, 2), &[100.0, 100.0], &[80.0, 80.0]));
        spec.figure_left = Some(value);
        spec.figure_top = Some(value);
        let (layout, _) = drawn(&spec);
        assert!(layout.size.0 < 30_000 && layout.size.1 < 30_000);
    }
}

// ---------------------------------------------------------------------------
// Free placement (#398)
// ---------------------------------------------------------------------------

use stokesforge_core::compose;
use stokesforge_plot::{FreeFrame, LayerPlacement};

/// A free graph of empty cells: `(row, column, left, top, width, height)`.
/// An empty cell shows the default axes: 76 pt on the left, 64 pt below, 12 pt
/// above and on the right.
fn free(title: &str, cells: &[(usize, usize, f64, f64, f64, f64)]) -> PlotSpec {
    let panels: Vec<PanelSpec> = cells
        .iter()
        .map(|&(row, column, left, top, width, height)| PanelSpec {
            free_frame: Some(FreeFrame {
                left,
                top,
                width,
                height,
            }),
            ..PanelSpec::at(row, column)
        })
        .collect();
    PlotSpec {
        layer_placement: LayerPlacement::Free,
        ..PlotSpec::empty(title).with_panels(Panels::new(panels))
    }
}

fn two_cells() -> PlotSpec {
    free(
        "",
        &[
            (0, 0, 100.0, 50.0, 300.0, 200.0),
            (0, 1, 600.0, 80.0, 200.0, 150.0),
        ],
    )
}

/// The image is the bounding box of the outer rectangles (frame plus its own
/// bands): x 24..812, y 38..314 in the stored coordinates.
#[test]
fn a_free_figure_is_the_bounding_box_of_its_outer_rectangles() {
    let layout = layout_of(&two_cells());
    assert_eq!(layout.placement, LayerPlacement::Free);
    assert!(layout.is_explicit());
    assert_eq!(layout.size, (788, 276));
    assert_eq!(layout.title_band, 0);
    assert_eq!(layout.cell_rect(0, 0), Some((0, 0, 388, 276)));
    assert_eq!(layout.frame_rect(0, 0), Some((76, 12, 300, 200)));
    assert_eq!(layout.cell_rect(0, 1), Some((500, 30, 288, 226)));
    assert_eq!(layout.frame_rect(0, 1), Some((576, 42, 200, 150)));
    assert_eq!(layout.free_origin, (24.0, 38.0));
    assert!(layout.column_breaks.is_empty() && layout.row_breaks.is_empty());
    assert!(layout.column_gutters.is_empty() && layout.frame_widths.is_empty());
    assert_eq!(layout.frame_rect(1, 0), None, "no such cell");
}

/// What is drawn is what the layout says.
#[test]
fn a_free_figure_is_drawn_where_its_layout_says() {
    let (layout, plot) = drawn(&two_cells());
    assert_eq!(
        frames(&plot),
        vec![((76, 12), (300, 200)), ((576, 42), (200, 150))]
    );
    assert_eq!(
        cells(&plot),
        vec![((0, 0), (388, 276)), ((500, 30), (288, 226))]
    );
    assert_eq!(layout.size, (788, 276));
}

/// Moving a cell to the left of the others grows the image on that side; the
/// cell that did not move keeps its place relative to the others. Witness: the
/// origin taken from the frames instead of the outer rectangles.
#[test]
fn moving_a_cell_left_grows_the_image_to_the_left() {
    let before = free(
        "",
        &[
            (0, 0, 0.0, 50.0, 300.0, 200.0),
            (0, 1, 600.0, 50.0, 200.0, 200.0),
        ],
    );
    let after = free(
        "",
        &[
            (0, 0, -100.0, 50.0, 300.0, 200.0),
            (0, 1, 600.0, 50.0, 200.0, 200.0),
        ],
    );
    let (b, a) = (layout_of(&before), layout_of(&after));
    assert_eq!(a.size.0, b.size.0 + 100);
    assert_eq!(a.size.1, b.size.1);
    let (b0, a0) = (b.frame_rect(0, 0).unwrap(), a.frame_rect(0, 0).unwrap());
    let (b1, a1) = (b.frame_rect(0, 1).unwrap(), a.frame_rect(0, 1).unwrap());
    assert_eq!(
        a0, b0,
        "the moved cell is the leftmost: it stays at the edge"
    );
    assert_eq!(a1.0, b1.0 + 100, "the other one is 100 pt further right");
    assert_eq!(a1.0 - a0.0, (b1.0 - b0.0) + 100);
    assert_eq!((a1.1, a1.2, a1.3), (b1.1, b1.2, b1.3));
}

/// A graph title adds its band on top and changes no width. Witness: the band
/// not taken off the cells' tops.
#[test]
fn the_graph_title_adds_its_band_on_top_of_a_free_figure() {
    let bare = layout_of(&two_cells());
    let mut titled_spec = two_cells();
    titled_spec.title = "Title".to_string();
    let titled = layout_of(&titled_spec);
    assert!(titled.title_band > 0);
    assert_eq!(titled.size.0, bare.size.0);
    assert_eq!(titled.size.1, bare.size.1 + titled.title_band);
    let (b, t) = (
        bare.frame_rect(0, 1).unwrap(),
        titled.frame_rect(0, 1).unwrap(),
    );
    assert_eq!(t.1, b.1 + titled.title_band);
    let plot = draw_at(&titled_spec, titled.size);
    assert_eq!(frames(&plot)[1].0, (t.0 as i32, t.1 as i32));
}

/// Left and Top of the figure are kept stored and ignored in free placement.
/// Witness: applied.
#[test]
fn the_figure_left_and_top_do_nothing_in_free_placement() {
    let plain = layout_of(&two_cells());
    let mut spec = two_cells();
    spec.figure_left = Some(300.0);
    spec.figure_top = Some(200.0);
    assert_eq!(layout_of(&spec), plain);
}

fn with_right_axis(mut spec: PlotSpec, column: NodeId, cell: usize) -> PlotSpec {
    let mut series = SeriesSpec::new(column);
    series.axis = SeriesAxis::Right;
    let panel = spec.panels.get_mut(cell).expect("panel");
    panel.right.visible = true;
    panel.series.push(series);
    spec
}

/// Fixed (#405) in free placement: each cell reserves one axis on each side, so
/// adding a right axis changes neither the image nor any frame. Witness: the
/// reserve floors removed.
#[test]
fn a_fixed_free_figure_keeps_its_size_when_an_axis_comes_in() {
    let (_, column) = project_with_column();
    let mut bare = two_cells();
    bare.fixed_figure = true;
    let with_axis = with_right_axis(bare.clone(), column, 0);
    let (a, b) = (layout_of(&bare), layout_of(&with_axis));
    assert_eq!(b.size, a.size);
    assert_eq!(b.frame_rect(0, 0), a.frame_rect(0, 0));
    assert_eq!(b.frame_rect(0, 1), a.frame_rect(0, 1));
    assert!(!b.fixed_overflow);
    // The reserve is the same on the four sides of every cell: 76 pt left and
    // right, 64 pt above and below.
    assert_eq!(a.frame_rect(0, 0).map(|f| f.0), Some(76));
    // From the left reserve of the first cell to the right reserve of the last.
    assert_eq!(a.size.0, 76 + (800 - 100) + 76);
    let plot = draw_at(&with_axis, b.size);
    assert_eq!(frames(&plot)[0], ((76, 64), (300, 200)));
}

/// An axis beyond the reserve, or a title, still grows a fixed figure and is
/// reported.
#[test]
fn a_title_grows_a_fixed_free_figure_and_is_reported() {
    let mut spec = two_cells();
    spec.fixed_figure = true;
    assert!(!layout_of(&spec).fixed_overflow);
    spec.title = "Title".to_string();
    assert!(layout_of(&spec).fixed_overflow);
}

/// A grid on 2 x 2 with a title and a right axis goes to free placement
/// without anything moving: same size, same frame in every cell. The way back
/// gives the same rows, columns and frame vectors.
#[test]
fn grid_to_free_and_back_keeps_the_figure_and_the_grid() {
    let (_, column) = project_with_column();
    let mut grid = framed(
        with_right_axis(grid("Title", 2, 2), column, 1),
        &[300.0, 200.0],
        &[180.0, 120.0],
    );
    grid.panels.get_mut(2).expect("panel").title = "Panel".to_string();
    let grid_layout = layout_of(&grid);
    let probe = mute((1, 1)).into_drawing_area();
    let frames_out = pipeline::free_frames_from_grid(&grid, &probe)
        .expect("layout")
        .expect("a grid with stored frames");
    let mut spec = grid.clone();
    assert!(compose::set_free_placement(&mut spec, &frames_out));
    let free_layout = layout_of(&spec);
    assert_eq!(free_layout.size, grid_layout.size);
    for (row, column) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        assert_eq!(
            free_layout.frame_rect(row, column),
            grid_layout.frame_rect(row, column),
            "cell ({row}, {column})"
        );
    }
    assert!(compose::set_grid_placement(&mut spec));
    assert_eq!(spec.rows(), grid.rows());
    assert_eq!(spec.columns(), grid.columns());
    assert_eq!(spec.frame_widths, grid.frame_widths);
    assert_eq!(spec.frame_heights, grid.frame_heights);
    assert_eq!(
        spec.panels
            .iter()
            .map(|p| (p.row, p.column))
            .collect::<Vec<_>>(),
        grid.panels
            .iter()
            .map(|p| (p.row, p.column))
            .collect::<Vec<_>>()
    );
    assert_eq!(layout_of(&spec), grid_layout);
}

#[test]
fn a_legacy_graph_has_no_free_frames_to_give() {
    let probe = mute((1, 1)).into_drawing_area();
    assert_eq!(
        pipeline::free_frames_from_grid(&grid("", 1, 2), &probe).expect("layout"),
        None
    );
    assert_eq!(
        pipeline::free_frames_from_grid(&two_cells(), &probe).expect("layout"),
        None,
        "already free"
    );
}

/// A free graph is never legacy: its frames are not to be fixed.
#[test]
fn a_free_graph_has_no_legacy_frames() {
    let probe = mute((1, 1)).into_drawing_area();
    assert_eq!(
        pipeline::legacy_frames(&two_cells(), &probe).expect("layout"),
        None
    );
}

/// Overlays share the frame of their base and do not make a cell of their own.
#[test]
fn an_overlay_layer_shares_the_cell_of_its_base() {
    let mut spec = two_cells();
    let overlay = PanelSpec::overlay_on(&spec.panels[0], OverlaySpec::new(true, true));
    let mut panels: Vec<PanelSpec> = spec.panels.iter().cloned().collect();
    panels.insert(1, overlay);
    spec.panels = Panels::new(panels);
    let layout = layout_of(&spec);
    assert_eq!(layout.free_cells.len(), 2);
    assert_eq!(
        layout.free_cells[1].base, 2,
        "keyed by the base layer index"
    );
    let plot = draw_at(&spec, layout.size);
    assert_eq!(plot.panels.len(), 3);
    assert_eq!(plot.panels[0].pixel_origin, plot.panels[1].pixel_origin);
}

/// Hostile identities (beyond the grid limits) do not confuse the cells.
#[test]
fn free_cells_are_keyed_by_identity_not_by_grid_position() {
    let spec = free(
        "",
        &[
            (0, 0, 0.0, 0.0, 200.0, 100.0),
            (usize::MAX - 1, 5, 400.0, 0.0, 200.0, 100.0),
        ],
    );
    let layout = layout_of(&spec);
    assert_eq!(layout.free_cells.len(), 2);
    assert_eq!(layout.frame_rect(usize::MAX - 1, 5).map(|f| f.2), Some(200));
    let plot = draw_at(&spec, layout.size);
    assert_eq!(plot.omitted_panels, 0);
}

#[test]
fn a_layer_goes_beside_a_free_cell_without_overlapping() {
    let spec = two_cells();
    let probe = mute((1, 1)).into_drawing_area();
    // Right of cell 0: its outer rectangle ends at x 412; the next cell's
    // outer rectangle starts at 524, which the new one (frame + 88 pt of
    // bands) would hit, so it goes past it: outer left 812, frame left 888.
    let right = pipeline::place_beside(&spec, &probe, 0, pipeline::BesideSide::Right)
        .expect("layout")
        .expect("a free graph");
    assert_eq!(
        (right.left, right.top, right.width, right.height),
        (888.0, 50.0, 300.0, 200.0)
    );
    // Below cell 0: same left, under its outer rectangle (bottom at 314).
    let below = pipeline::place_beside(&spec, &probe, 0, pipeline::BesideSide::Below)
        .expect("layout")
        .expect("a free graph");
    assert_eq!(
        (below.left, below.top, below.width, below.height),
        (100.0, 326.0, 300.0, 200.0)
    );
    assert_eq!(
        pipeline::place_beside(&grid("", 1, 1), &probe, 0, pipeline::BesideSide::Right)
            .expect("layout"),
        None
    );
}

/// A panel title takes a band inside its cell's outer rectangle, above the
/// frame: the frame does not move, the cell grows upward (and the image with
/// it when it is the topmost). Witness: the band left out of the top of the
/// outer rectangle.
#[test]
fn a_panel_title_grows_its_free_cell_upward_and_leaves_the_frame_alone() {
    let mut spec = free("", &[(0, 0, 100.0, 80.0, 300.0, 200.0)]);
    let bare = layout_of(&spec);
    spec.panels.get_mut(0).expect("panel").title = "Panel".to_string();
    let titled = layout_of(&spec);
    let band = titled.size.1 - bare.size.1;
    assert!(band > 0);
    assert_eq!(titled.size.0, bare.size.0);
    assert_eq!(
        titled.frame_rect(0, 0).map(|f| (f.2, f.3)),
        Some((300, 200))
    );
    assert_eq!(
        titled.frame_rect(0, 0).map(|f| f.1),
        bare.frame_rect(0, 0).map(|f| f.1 + band)
    );
    let plot = draw_at(&spec, titled.size);
    let f = titled.frame_rect(0, 0).unwrap();
    assert_eq!(frames(&plot)[0], ((f.0 as i32, f.1 as i32), (f.2, f.3)));
}
