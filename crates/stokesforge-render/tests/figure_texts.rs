//! The sentences the renderer draws by itself come from [`FigureTexts`]:
//! legend placeholder, « … and N more » line, name of a row series (#369).

use std::cell::RefCell;
use std::rc::Rc;

use plotters_backend::{
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingBackend, DrawingErrorKind,
};
use stokesforge_core::column::Column;
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render::text::{estimate, TextMeasurer};
use stokesforge_render::{pipeline, FigureTexts};

/// A backend that records the text it is asked to draw.
struct Recording {
    measurer: TextMeasurer,
    drawn: Rc<RefCell<Vec<String>>>,
}

#[derive(Debug)]
enum Never {}

impl std::fmt::Display for Never {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for Never {}

impl DrawingBackend for Recording {
    type ErrorType = Never;

    fn get_size(&self) -> (u32, u32) {
        (640, 480)
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
        text: &str,
        _style: &S,
        _pos: BackendCoord,
    ) -> Result<(), DrawingErrorKind<Never>> {
        self.drawn.borrow_mut().push(text.to_owned());
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

/// A fake language that cannot be mistaken for the default.
fn custom_texts() -> FigureTexts {
    FigureTexts::new(
        "xx",
        "(xx-placeholder)",
        |name| format!("{name} xx-named"),
        |count| format!("xx-more {count}"),
        |number| format!("xx-row {number}"),
    )
}

/// One panel with `unnamed` series on live columns, then each series of
/// `extra` (built from the ids), drawn with `texts` (`None` = plain `draw`).
fn drawn_texts(
    unnamed: usize,
    texts: Option<&FigureTexts>,
    extra: impl Fn(&mut Project, NodeId) -> Vec<SeriesSpec>,
) -> Vec<String> {
    let mut project = Project::new("Texts").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("sheet");
    let mut panel = PanelSpec::at(0, 0);
    for index in 0..unnamed {
        let id = project
            .add_column(
                sheet,
                None,
                Some(&format!("S{index}")),
                Column::from_f64(&[1.0, 2.0, 3.0]),
            )
            .expect("column");
        panel.series.push(SeriesSpec::new(id));
    }
    panel.series.extend(extra(&mut project, sheet));
    let spec = PlotSpec::empty("Test").with_panels(panel);
    let plot = resolve_spec(project.tree(), &spec);
    let drawn = Rc::new(RefCell::new(Vec::new()));
    let backend = Recording {
        measurer: TextMeasurer::default(),
        drawn: Rc::clone(&drawn),
    };
    match texts {
        Some(texts) => pipeline::draw_with_texts(backend, &plot, texts),
        None => pipeline::draw(backend, &plot),
    }
    .expect("drawing");
    let recorded = drawn.borrow().clone();
    recorded
}

/// A series whose Y column is deleted afterwards: a broken reference.
fn broken_series(project: &mut Project, sheet: NodeId, name: &str) -> SeriesSpec {
    let id = project
        .add_column(sheet, None, Some("Gone"), Column::from_f64(&[1.0]))
        .expect("column");
    let mut series = SeriesSpec::new(id);
    series.name = name.to_owned();
    project.remove(id).expect("deleted");
    series
}

fn contains(texts: &[String], needle: &str) -> bool {
    texts.iter().any(|text| text.contains(needle))
}

#[test]
fn a_full_legend_says_how_many_entries_it_left_out_in_the_given_language() {
    let by_default = drawn_texts(26, None, |_, _| Vec::new());
    assert!(contains(&by_default, "… and 2 more"), "{by_default:?}");

    let custom = custom_texts();
    let drawn = drawn_texts(26, Some(&custom), |_, _| Vec::new());
    assert!(contains(&drawn, "xx-more 2"), "{drawn:?}");
    assert!(!contains(&drawn, "and 2 more"), "{drawn:?}");
}

#[test]
fn a_broken_reference_is_annotated_in_the_given_language() {
    let custom = custom_texts();
    let drawn = drawn_texts(0, Some(&custom), |project, sheet| {
        vec![
            broken_series(project, sheet, ""),
            broken_series(project, sheet, "N"),
        ]
    });
    assert!(contains(&drawn, "(xx-placeholder)"), "{drawn:?}");
    assert!(contains(&drawn, "N xx-named"), "{drawn:?}");

    let by_default = drawn_texts(0, None, |project, sheet| {
        vec![
            broken_series(project, sheet, ""),
            broken_series(project, sheet, "N"),
        ]
    });
    assert!(
        contains(&by_default, "(broken reference)"),
        "{by_default:?}"
    );
    assert!(
        contains(&by_default, "N — broken reference"),
        "{by_default:?}"
    );
}

#[test]
fn an_unnamed_row_series_takes_its_name_from_the_given_language() {
    let custom = custom_texts();
    let drawn = drawn_texts(1, Some(&custom), |_, _| Vec::new());
    assert!(
        !contains(&drawn, "xx-row"),
        "a column series is no row series"
    );

    let row_series = |project: &mut Project, sheet: NodeId| {
        // Two columns with an x': the row series needs points to be drawn.
        let mut anchor = None;
        for (name, x_prime) in [("A", 0.0), ("B", 10.0)] {
            let id = project
                .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0]))
                .expect("column");
            let mut meta = project
                .tree()
                .node(id)
                .unwrap()
                .as_column()
                .unwrap()
                .meta()
                .clone();
            meta.x_prime = Some(x_prime);
            project.set_column_meta(id, meta).expect("x'");
            anchor.get_or_insert(id);
        }
        let anchor = anchor.unwrap();
        vec![SeriesSpec {
            row: Some(1),
            ..SeriesSpec::new(anchor)
        }]
    };
    let drawn = drawn_texts(0, Some(&custom), row_series);
    assert!(contains(&drawn, "xx-row 2"), "{drawn:?}");
    let by_default = drawn_texts(0, None, row_series);
    assert!(contains(&by_default, "Row 2"), "{by_default:?}");
}
