//! Scratch temporaire — reproductions visuelles du rapport de bugs
//! 2026-09-23 (contour, panneaux, redimensionnement du graphe). À SUPPRIMER
//! après usage.

use plotyx_core::{Column, Project};
use plotyx_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_session::{Session, WindowGeometry};

#[test]
fn genere_repro_contour_colle_au_pm() {
    let mut project = Project::new("Repro").unwrap();
    let root = project.root();
    let book = project.add_empty_workbook(root, Some("Book1")).unwrap();

    let session = Session::spawn(project, None).unwrap();
    session
        .handle()
        .set_window_geometry(
            book,
            WindowGeometry {
                x: 50.0,
                y: 80.0,
                width: 500.0,
                height: 400.0,
                view: None,
            },
        )
        .unwrap();
    session
        .handle()
        .save(std::path::Path::new("/tmp/repro_contour.plotyx"))
        .unwrap();
    session.shutdown().unwrap();
}

#[test]
fn genere_repro_graphe_petite_fenetre() {
    let mut project = Project::new("Repro").unwrap();
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .unwrap()
        .unwrap();
    let xs: Vec<f64> = (0..=20).map(|i| i as f64).collect();
    let ys: Vec<f64> = xs.iter().map(|x| x * x).collect();
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&xs))
        .unwrap();
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&ys))
        .unwrap();

    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "x".into(),
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "y".into(),
        ..AxisSpec::default()
    };
    let mut serie = SeriesSpec::new_xy(x, y);
    serie.line = LineSpec {
        color: "#4c78a8".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(serie);
    let spec = PlotSpec {
        title: "Test".into(),
        background: "#ffffff".into(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel);
    let graph = project.add_graph(root, Some("Graph1"), spec).unwrap();

    let session = Session::spawn(project, None).unwrap();
    session
        .handle()
        .set_window_geometry(
            graph,
            WindowGeometry {
                x: 20.0,
                y: 20.0,
                width: 260.0,
                height: 200.0,
                view: None,
            },
        )
        .unwrap();
    session
        .handle()
        .save(std::path::Path::new("/tmp/repro_graphe_petit.plotyx"))
        .unwrap();
    session.shutdown().unwrap();

    let mut project2 = Project::new("Repro").unwrap();
    let root2 = project2.root();
    let book2 = project2.add_workbook(root2, Some("Book1")).unwrap();
    let sheet2 = project2
        .tree()
        .child_by_name(book2, "Sheet1")
        .unwrap()
        .unwrap();
    let x2 = project2
        .add_column(sheet2, None, Some("X"), Column::from_f64(&xs))
        .unwrap();
    let y2 = project2
        .add_column(sheet2, None, Some("Y"), Column::from_f64(&ys))
        .unwrap();
    let mut panel2 = PanelSpec::at(0, 0);
    let mut serie2 = SeriesSpec::new_xy(x2, y2);
    serie2.line = LineSpec {
        color: "#4c78a8".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel2.series.push(serie2);
    let spec2 = PlotSpec {
        title: "Test".into(),
        background: "#ffffff".into(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel2);
    let graph2 = project2.add_graph(root2, Some("Graph1"), spec2).unwrap();
    let session2 = Session::spawn(project2, None).unwrap();
    session2
        .handle()
        .set_window_geometry(
            graph2,
            WindowGeometry {
                x: 20.0,
                y: 20.0,
                width: 1200.0,
                height: 900.0,
                view: None,
            },
        )
        .unwrap();
    session2
        .handle()
        .save(std::path::Path::new("/tmp/repro_graphe_grand.plotyx"))
        .unwrap();
    session2.shutdown().unwrap();
}
