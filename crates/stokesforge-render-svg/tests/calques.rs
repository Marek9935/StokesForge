//! Calques superposés (#167) vus dans l'export : le second calque porte son
//! axe X en haut et son axe Y à droite, avec leurs graduations propres.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{OverlaySpec, PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

fn exporte(overlay: Option<OverlaySpec>) -> String {
    let mut project = Project::new("Calques").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let mut colonne = |nom: &str, valeurs: Vec<f64>| -> NodeId {
        let column = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(valeurs),
            ColumnMeta::default(),
        )
        .expect("colonne");
        project
            .add_column(sheet, None, Some(nom), column)
            .expect("colonne")
    };
    let x = colonne("X", vec![0.0, 1.0, 2.0]);
    let y = colonne("Y", vec![10.0, 20.0, 30.0]);
    let x2 = colonne("X2", vec![400.0, 600.0, 800.0]);
    let y2 = colonne("Y2", vec![0.0, 0.5, 1.0]);
    let mut base = PanelSpec::at(0, 0);
    base.series.push(SeriesSpec::new_xy(x, y));
    let mut panels = vec![base.clone()];
    if let Some(overlay) = overlay {
        let mut haut = PanelSpec::overlay_on(&base, overlay);
        haut.series.push(SeriesSpec::new_xy(x2, y2));
        panels.push(haut);
    }
    let spec = PlotSpec::empty("").with_panels(Panels::new(panels));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    if let Some(dir) = std::env::var_os("STOKESFORGE_DUMP") {
        let nom = if overlay.is_some() { "calques" } else { "base" };
        std::fs::write(std::path::Path::new(&dir).join(format!("{nom}.svg")), &svg).ok();
    }
    svg
}

/// Les graduations 400…800 du calque superposé n'existent que dans l'export
/// qui le contient : témoin sans calque.
#[test]
fn le_calque_superpose_trace_ses_graduations_en_haut() {
    let sans = exporte(None);
    assert!(!sans.contains("800.0"), "témoin : pas de graduation 800");
    let avec = exporte(Some(OverlaySpec {
        x_top: true,
        y_right: true,
    }));
    assert!(avec.contains("800.0"), "graduation X2 absente : {avec}");
}

/// #166 : une série « ligne » se trace contre les x' des colonnes — les
/// graduations de l'axe X couvrent les instants (0…240), pas les indices.
#[test]
fn une_serie_ligne_se_trace_contre_les_x_prime() {
    let mut project = Project::new("Cinétique").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let mut ancre = None;
    for (temps, valeur) in [(0.0, 1.0), (120.0, 2.0), (240.0, 3.0)] {
        let id = project
            .add_column(sheet, None, None, Column::from_f64(&[valeur, valeur * 2.0]))
            .expect("colonne");
        let mut meta = project
            .tree()
            .node(id)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .clone();
        meta.x_prime = Some(temps);
        project.set_column_meta(id, meta).expect("x'");
        ancre.get_or_insert(id);
    }
    let mut serie = SeriesSpec::new(ancre.unwrap());
    serie.row = Some(1);
    let mut panel = PanelSpec::at(0, 0);
    panel.series.push(serie);
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    assert!(svg.contains("200.0"), "graduation x' absente : {svg}");
    assert!(
        svg.contains("<polyline") || svg.contains("<path"),
        "courbe absente"
    );
}
