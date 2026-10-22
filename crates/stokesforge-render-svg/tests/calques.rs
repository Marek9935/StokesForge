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

/// Les graduations 400…800 (écrites sans décimale, #175) du calque superposé n'existent que dans l'export
/// qui le contient : témoin sans calque.
#[test]
fn le_calque_superpose_trace_ses_graduations_en_haut() {
    let sans = exporte(None);
    assert!(!sans.contains("\n800\n"), "témoin : pas de graduation 800");
    let avec = exporte(Some(OverlaySpec {
        x_top: true,
        y_right: true,
    }));
    assert!(avec.contains("\n800\n"), "graduation X2 absente : {avec}");
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
    assert!(svg.contains("\n200\n"), "graduation x' absente : {svg}");
    assert!(
        svg.contains("<polyline") || svg.contains("<path"),
        "courbe absente"
    );
}

/// Les axes tracés se rejoignent aux coins : Plotters les laisse à un pixel
/// l'un de l'autre, le pipeline comble ce coin vide.
#[test]
fn les_axes_se_rejoignent_aux_coins() {
    let coins = |svg: &str| {
        svg.lines()
            .filter(|l| l.contains("<polyline"))
            .filter_map(|l| l.split("points=\"").nth(1))
            .filter(|p| {
                let n: Vec<f64> = p
                    .split([',', ' ', '"'])
                    .filter_map(|v| v.parse().ok())
                    .collect();
                n.len() == 6
                    && (n[0] - n[2]).abs() == 1.0
                    && n[1] == n[3]
                    && n[2] == n[4]
                    && (n[3] - n[5]).abs() == 1.0
            })
            .count()
    };
    assert_eq!(coins(&exporte(None)), 1, "bas-gauche seul");
    let avec = exporte(Some(OverlaySpec {
        x_top: true,
        y_right: true,
    }));
    assert_eq!(coins(&avec), 4);
}

/// #168 : trois calques superposés, axes empilés vers l'extérieur. Retourne
/// l'export de la figure.
fn exporte_trois() -> String {
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
    let jeux = [
        ("X", vec![0.0, 1.0, 2.0]),
        ("Y", vec![10.0, 20.0, 30.0]),
        ("X2", vec![400.0, 600.0, 800.0]),
        ("Y2", vec![0.0, 0.5, 1.0]),
        ("X3", vec![4000.0, 6000.0, 8000.0]),
        ("Y3", vec![100.0, 200.0, 300.0]),
        ("X4", vec![40000.0, 60000.0, 80000.0]),
        ("Y4", vec![1000.0, 2000.0, 3000.0]),
    ];
    let ids: Vec<NodeId> = jeux
        .into_iter()
        .map(|(nom, valeurs)| colonne(nom, valeurs))
        .collect();
    let mut base = PanelSpec::at(0, 0);
    base.series.push(SeriesSpec::new_xy(ids[0], ids[1]));
    let mut panels = vec![base.clone()];
    for rang in 1..4 {
        let mut calque = PanelSpec::overlay_on(&base, OverlaySpec::SECOND_AXES);
        calque
            .series
            .push(SeriesSpec::new_xy(ids[2 * rang], ids[2 * rang + 1]));
        panels.push(calque);
    }
    let spec = PlotSpec::empty("").with_panels(Panels::new(panels));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    if let Some(dir) = std::env::var_os("STOKESFORGE_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join("trois.svg"), &svg).ok();
    }
    svg
}

#[test]
fn les_graduations_des_calques_superposes_s_empilent() {
    let svg = exporte_trois();
    for graduation in ["\n850\n", "\n8500\n", "\n85000\n", "\n3000\n"] {
        assert!(svg.contains(graduation), "{graduation:?} absente : {svg}");
    }
    // Les graduations X3 et X4 sont au-dessus de celles de X2 (y plus petit).
    let ordonnee = |texte: &str| -> f64 {
        let fin = svg.find(&format!("\n{texte}\n")).expect("graduation");
        let balise = &svg[svg[..fin].rfind("<text").expect("balise")..fin];
        let y = balise.split("y=\"").nth(1).expect("y");
        y.split('"').next().unwrap().parse().expect("nombre")
    };
    assert!(ordonnee("8500") < ordonnee("850"));
    assert!(ordonnee("85000") < ordonnee("8500"));
}
