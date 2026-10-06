//! Régression #150 : le spectre réel reste fidèle au dézoom, sans lisser
//! ses données. Les exports optionnels permettent de regarder les cadrages.

use egui::{Color32, Pos2, Rect, Shape};
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{resolve::resolve_spec, Project};
use stokesforge_import::{DecimalStyle, HeaderSkip, ImportRecipe, Separator};
use stokesforge_plot::{AxisScale, PanelSpec, PlotSpec, SeriesAxis, SeriesSpec};
use stokesforge_render::{PanelTransform, TextMeasurer};

#[test]
fn jasco_garde_les_variations_sous_pixel_aux_deux_cadrages() {
    let bytes = include_bytes!("../../../test/import/JascoUV_vis.csv");
    let mut recipe = ImportRecipe::new("Jasco UV-Vis");
    recipe.separator = Separator::Semicolon;
    recipe.decimal = DecimalStyle::Comma;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    let table = stokesforge_import::parse(bytes, &recipe).unwrap();
    assert_eq!(table.row_count(), 551);
    let xs = &table.columns[0].values;
    let ys = &table.columns[1].values;
    let mut project = Project::new("Jasco #150").unwrap();
    let book = project.add_empty_workbook(project.root(), None).unwrap();
    let sheet = project.add_worksheet(book, None).unwrap();
    let mut ids = Vec::new();
    for values in [xs, ys] {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values.clone()),
            ColumnMeta::default(),
        )
        .unwrap();
        ids.push(project.add_column(sheet, None, None, column).unwrap());
    }
    let measurer = TextMeasurer::default();
    let mut errors = Vec::new();
    for (name, ranges, right) in [
        ("dezoom", ((250.0, 800.0), (-0.02, 1.0)), false),
        ("zoom", ((400.0, 550.0), (0.0, 0.18)), false),
        ("log_droite", ((250.0, 800.0), (-0.02, 1.0)), true),
    ] {
        let mut panel = PanelSpec::at(0, 0);
        panel.legend.visible = false;
        panel.bottom.auto_range = false;
        panel.left.auto_range = false;
        panel.bottom.range_min = Some(ranges.0 .0);
        panel.bottom.range_max = Some(ranges.0 .1);
        panel.left.range_min = Some(ranges.1 .0);
        panel.left.range_max = Some(ranges.1 .1);
        let mut series = SeriesSpec::new_xy(ids[0], ids[1]);
        if right {
            panel.bottom.scale = AxisScale::Log;
            panel.right = panel.left.clone();
            series.axis = SeriesAxis::Right;
        }
        series.line.color = "#4c78a8".into();
        panel.series.push(series);
        let spec = PlotSpec {
            background: "#ffffff".into(),
            ..PlotSpec::empty("").with_panels(panel)
        };
        let plot = resolve_spec(project.tree(), &spec);
        let mut shapes = Vec::new();
        let drawn = stokesforge_render_egui::collect_shapes(
            &mut shapes,
            Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 500.0)),
            &measurer,
            &plot,
        )
        .unwrap();
        assert_eq!(drawn.decimated_points, 0, "les 551 mesures sont conservées");
        let (svg, exported) =
            stokesforge_render_svg::render_to_string_with_geometry(&plot, (800, 500), &measurer)
                .unwrap();
        assert_eq!(
            drawn.panels[0].series_geometry,
            exported.panels[0].series_geometry
        );
        if let Ok(dir) = std::env::var("STOKESFORGE_JASCO_CAPTURE") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(format!("{dir}/{name}.svg"), &svg).unwrap();
            std::fs::write(
                format!("{dir}/{name}.png"),
                stokesforge_render_svg::png::document_to_png(&svg, 2.0).unwrap(),
            )
            .unwrap();
        }
        let transform = PanelTransform::of_axis(&drawn.panels[0], right).unwrap();
        let vertices: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Path(path)
                    if path.stroke.color
                        == egui::epaint::ColorMode::Solid(Color32::from_rgb(76, 120, 168)) =>
                {
                    Some(&path.points)
                }
                _ => None,
            })
            .flatten()
            .collect();
        // Comparer aussi les sommets des sorties réelles : une égalité de
        // DrawnPanel seule ne prouverait pas le décodage dans les backends.
        let svg_vertices: Vec<(f64, f64)> = svg
            .lines()
            .filter(|line| line.starts_with("<polyline") && line.contains("stroke=\"#4C78A8\""))
            .flat_map(|line| {
                let points = line
                    .split_once("points=\"")
                    .unwrap()
                    .1
                    .split('"')
                    .next()
                    .unwrap();
                points
                    .split_whitespace()
                    .map(|point| {
                        let (x, y) = point.split_once(',').unwrap();
                        (x.parse().unwrap(), y.parse().unwrap())
                    })
                    .collect::<Vec<(f64, f64)>>()
            })
            .collect();
        assert_eq!(vertices.len(), svg_vertices.len());
        assert!(!vertices.is_empty());
        for (screen, svg) in vertices.iter().zip(&svg_vertices) {
            assert_eq!((f64::from(screen.x), f64::from(screen.y)), *svg);
        }
        let mut max_error = 0.0_f64;
        for (&x, &y) in xs.iter().zip(ys) {
            let (px, py) = transform.data_to_pixel(x, y).unwrap();
            // Plotters pose l'extrémité sur le dernier pixel inclus, soit
            // taille - 1 pour le repère principal. Son repère secondaire
            // emploie les bords du rectangle, comme la navigation.
            let panel = &drawn.panels[0];
            let fx = if right {
                (x.log10() - ranges.0 .0.log10()) / (ranges.0 .1.log10() - ranges.0 .0.log10())
            } else {
                (x - ranges.0 .0) / (ranges.0 .1 - ranges.0 .0)
            };
            let px = px - if right { 0.0 } else { fx };
            let py = py
                - if right {
                    0.0
                } else {
                    1.0 - (y - ranges.1 .0) / (ranges.1 .1 - ranges.1 .0)
                };
            assert!(panel.pixel_size.0 > 1 && panel.pixel_size.1 > 1);
            if transform.contains(px, py) {
                let error = vertices
                    .iter()
                    .map(|p| (f64::from(p.x) - px).abs().max((f64::from(p.y) - py).abs()))
                    .fold(f64::INFINITY, f64::min);
                max_error = max_error.max(error);
            }
        }
        errors.push((name, max_error));
    }
    for (name, error) in errors {
        assert!(
            error <= 1.0 / 32.0 + 1e-4,
            "{name} : écart de {error} px au lieu d'au plus 1/32 px"
        );
    }
}
