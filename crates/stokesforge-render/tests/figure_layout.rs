//! `figure_layout` (#394): the single place where the sizes of the cells, the
//! title band and the page meet, and the cells the pipeline draws from it.

use plotters::drawing::IntoDrawingArea;
use plotters_backend::{
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{PanelSpec, Panels, PlotSpec};
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

fn draw_at(spec: &PlotSpec, size: (u32, u32)) -> DrawnPlot {
    let project = Project::new("Layout").expect("project");
    let plot = resolve_spec(project.tree(), spec);
    pipeline::draw(mute(size), &plot).expect("drawing")
}

/// The cells actually drawn, `(origin, size)`, by layer index.
fn cells(drawn: &DrawnPlot) -> Vec<((i32, i32), (u32, u32))> {
    drawn
        .panels
        .iter()
        .map(|p| (p.cell_origin, p.cell_size))
        .collect()
}

#[test]
fn a_legacy_graph_is_480_by_360_shared_equally_title_band_inside() {
    let spec = grid("Title", 1, 2);
    let layout = layout_of(&spec);
    assert_eq!(layout.size, (480, 360));
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

#[test]
fn explicit_sizes_make_the_figure_their_sum_plus_the_title_band() {
    let mut spec = grid("Title", 2, 2);
    spec.column_widths = vec![300.0, 200.0];
    spec.row_heights = vec![250.0, 100.0];
    let layout = layout_of(&spec);
    let band = layout.title_band;
    assert!(band > 0);
    assert_eq!(layout.size, (500, 350 + band));
    let drawn = draw_at(&spec, layout.size);
    let b = band as i32;
    assert_eq!(
        cells(&drawn),
        vec![
            ((0, b), (300, 250)),
            ((300, b), (200, 250)),
            ((0, b + 250), (300, 100)),
            ((300, b + 250), (200, 100)),
        ]
    );
    // `cell_rect` is what the readers use: the same rectangles.
    assert_eq!(layout.cell_rect(1, 1), Some((300, band + 250, 200, 100)));
}

#[test]
fn removing_the_title_shrinks_the_figure_by_its_band() {
    let mut spec = grid("Title", 1, 1);
    spec.column_widths = vec![480.0];
    spec.row_heights = vec![360.0];
    let titled = layout_of(&spec);
    spec.title.clear();
    let bare = layout_of(&spec);
    assert_eq!(bare.size, (480, 360));
    assert_eq!(titled.size.1, 360 + titled.title_band);
}

#[test]
fn breakpoints_are_rounded_prefix_sums_never_rounded_widths() {
    let mut spec = grid("", 1, 3);
    spec.column_widths = vec![100.4, 100.4, 100.4];
    let layout = layout_of(&spec);
    // 301.2 rounds to 301; each width alone would round to 100 (total 300).
    assert_eq!(layout.size.0, 301);
    assert_eq!(layout.column_breaks, vec![0, 100, 201, 301]);
    let drawn = draw_at(&spec, layout.size);
    let widths: Vec<u32> = cells(&drawn).iter().map(|(_, s)| s.0).collect();
    assert_eq!(widths.iter().sum::<u32>(), 301);
}

#[test]
fn a_grid_beyond_the_cell_cap_reads_the_legacy_split() {
    let mut spec = PlotSpec::empty("").with_panels(Panels::new(vec![
        PanelSpec::at(0, 0),
        PanelSpec::at(usize::MAX, 0),
    ]));
    spec.column_widths = vec![1000.0];
    spec.row_heights = vec![1000.0];
    assert_eq!(layout_of(&spec).size, (480, 360));
}

#[test]
fn hostile_sizes_draw_without_panicking() {
    let mut spec = grid("T", 2, 2);
    spec.column_widths = vec![f64::NAN, 1e300, -3.0, 5.0, 7.0, 9.0];
    spec.row_heights = vec![f64::INFINITY];
    let layout = layout_of(&spec);
    assert!(layout.size.0 <= 4000 && layout.size.1 <= 4000 + layout.title_band);
    let drawn = draw_at(&spec, layout.size);
    assert_eq!(drawn.omitted_panels, 0, "sanitized cells are never omitted");
}
