//! Axes miroirs (#176) vus dans l'export : l'échelle du haut et de droite
//! reprend celle du bas et de gauche, sans titre, et peut se limiter au trait.

use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PanelSpec, Panels, PlotSpec, SeriesSpec};
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::render_to_string;

fn exporte(nom: &str, regle: impl FnOnce(&mut PanelSpec)) -> String {
    let mut project = Project::new("Miroirs").expect("projet");
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
    let x = colonne("X", vec![0.0, 400.0, 800.0]);
    let y = colonne("Y", vec![10.0, 20.0, 30.0]);
    let mut panneau = PanelSpec::at(0, 0);
    panneau.bottom.label = "Temps".into();
    panneau.series.push(SeriesSpec::new_xy(x, y));
    regle(&mut panneau);
    let spec = PlotSpec::empty("").with_panels(Panels::new(vec![panneau]));
    let plot = resolve_spec(project.tree(), &spec);
    let svg = render_to_string(&plot, (480, 360), &TextMeasurer::default()).expect("export");
    if let Some(dir) = std::env::var_os("STOKESFORGE_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join(format!("{nom}.svg")), &svg).ok();
    }
    svg
}

fn occurrences(svg: &str, graduation: &str) -> usize {
    svg.matches(&format!("\n{graduation}\n")).count()
}

#[test]
fn les_axes_miroirs_repetent_les_graduations_sans_titre() {
    let sans = exporte("miroirs-sans", |_| {});
    let avec = exporte("miroirs-avec", |p| {
        p.top.mirror = true;
        p.right.mirror = true;
    });
    assert_eq!(occurrences(&avec, "800"), 2 * occurrences(&sans, "800"));
    assert_eq!(occurrences(&avec, "30"), 2 * occurrences(&sans, "30"));
    assert_eq!(
        avec.matches("Temps").count(),
        sans.matches("Temps").count(),
        "aucun titre sur les miroirs"
    );
}

#[test]
fn un_miroir_au_trait_seul_garde_les_tirets_sans_les_nombres() {
    let nus = exporte("miroirs-nus", |p| {
        p.top.mirror = true;
        p.top.tick_labels = false;
        p.right.mirror = true;
        p.right.tick_labels = false;
    });
    let sans = exporte("miroirs-sans", |_| {});
    assert_eq!(occurrences(&nus, "800"), occurrences(&sans, "800"));
    assert_eq!(occurrences(&nus, "30"), occurrences(&sans, "30"));
}
