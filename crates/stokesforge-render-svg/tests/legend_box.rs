//! The legend box of #507 in the exported document: frame, thickness, colour,
//! background and thumbnail length, from the same pipeline as the screen.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{LegendSpec, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

fn export(adjust: impl FnOnce(&mut LegendSpec)) -> String {
    let mut project = Project::new("Legend").expect("project");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("workbook");
    let sheet = project.add_worksheet(book, None).expect("worksheet");
    let mut add = |name: &str| {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(vec![0.0, 1.0, 2.0]),
            ColumnMeta::default(),
        )
        .expect("column");
        project
            .add_column(sheet, None, Some(name), column)
            .expect("column")
    };
    let (x, y) = (add("X"), add("Signal"));
    let mut panel = PanelSpec::at(0, 0);
    let mut series = SeriesSpec::new_xy(x, y);
    series.line.color = "#112233".to_string();
    panel.series.push(series);
    adjust(&mut panel.legend);
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    let plot = resolve_spec(project.tree(), &spec);
    render_to_string(&plot, (480, 360), &TextMeasurer::default())
        .expect("export")
        .to_uppercase()
}

/// The 77-wide rectangles (the legend's frame and background), as
/// `(fill, opacity)`.
fn rects(document: &str) -> Vec<(String, String)> {
    document
        .split("<RECT")
        .skip(1)
        .filter_map(|tag| {
            let tag = tag.split('>').next()?;
            let attr = |name: &str| {
                let key = format!(" {name}=\"");
                tag.split(&key)
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .map(str::to_string)
            };
            if attr("WIDTH")? != "77" {
                return None;
            }
            Some((attr("FILL")?, attr("OPACITY")?))
        })
        .collect()
}

/// The default legend draws its frame as a plain outline in the automatic
/// ink, as every file did before the box settings existed. Witness: a frame
/// that is no longer drawn by default, or in another ink, fails.
#[test]
fn the_default_legend_has_its_ink_frame() {
    let document = export(|_| {});
    assert!(
        document.contains("<RECT X=\"383\" Y=\"20\" WIDTH=\"77\" HEIGHT=\"27\" OPACITY=\"1\" FILL=\"NONE\" STROKE=\"#202020\"/>"),
        "{document}"
    );
}

/// Frame off draws no outline (the background is still painted); a frame of
/// 3 pt in red is a 3 pt red path. Witness: ignoring `frame`, the colour or
/// the thickness fails one of the three assertions.
#[test]
fn the_frame_can_be_hidden_and_thick_and_coloured() {
    let off = export(|legend| legend.frame = false);
    assert!(
        !off.contains(
            "<RECT X=\"383\" Y=\"20\" WIDTH=\"77\" HEIGHT=\"27\" OPACITY=\"1\" FILL=\"NONE\""
        ),
        "no outline rectangle without a frame"
    );
    assert!(off.contains("#112233"), "the thumbnail is still there");

    let thick = export(|legend| {
        legend.frame_color = "#ff0000".to_owned();
        legend.frame_width = Some(3.0);
    });
    assert!(
        thick.contains(
            "STROKE=\"#FF0000\" STROKE-WIDTH=\"3\" POINTS=\"421,20 460,20 460,47 383,47 383,20 421,20 \""
        ),
        "{thick}"
    );
}

/// The background: automatic is the panel's (transparent here, nothing
/// painted but the default fill), a colour with opacity is painted with it.
/// Witness: a background not read leaves the fill transparent.
#[test]
fn the_background_takes_a_colour_with_opacity() {
    let document = export(|legend| legend.background = "#ffff0080".to_owned());
    let fills = rects(&document);
    assert!(
        fills
            .iter()
            .any(|(fill, opacity)| fill == "#FFFF00" && opacity.starts_with("0.50")),
        "{fills:?}"
    );
    let plain = rects(&export(|_| {}));
    assert!(plain.iter().all(|(fill, _)| fill != "#FFFF00"));
}

/// A thumbnail of 50 pt is a 50 pt line (26 by default). Witness: a swatch
/// length not read leaves 26.
#[test]
fn the_swatch_length_is_the_length_of_the_stub() {
    let default = export(|_| {});
    assert!(default.contains("POINTS=\"389,33 415,33 \""), "26 pt");
    let long = export(|legend| legend.swatch_length = Some(50.0));
    assert!(long.contains("POINTS=\"365,33 415,33 \""), "50 pt");
}
