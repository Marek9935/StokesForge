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
