//! Rendu **visible** hors écran — tâche R4.6, moitié export.
//!
//! ```sh
//! cargo run -p plotyx-render-svg --example figure          # ./spectre.svg
//! cargo run -p plotyx-render-svg --example figure /tmp/a.svg
//! ```
//!
//! Le séquencement « le contrat d'abord » a un défaut connu : cinq semaines
//! sans rien voir bouger. Sur un chantier de rendu, le retour visuel n'est
//! pas un confort — c'est ce qui fait remonter les erreurs qu'aucun test ne
//! formule (`roadmap-rendu.md` §4.2). Cet exemple est la moitié qui se
//! regarde **sans serveur graphique** ; l'autre moitié est la fenêtre du
//! banc d'essai (`cargo run -p plotyx-app`).
//!
//! Le graphe est le même que celui du test d'accord, à dessein : ce qu'on
//! regarde est exactement ce que le test compare.

use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::resolve::resolve_spec;
use plotyx_core::Project;
use plotyx_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_render::TextMeasurer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cible = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "spectre.svg".into());

    let mut project = Project::new("Démonstration")?;
    let root = project.root();
    let book = project.add_empty_workbook(root, Some("Book1"))?;
    let sheet = project.add_worksheet(book, Some("Sheet1"))?;

    let xs: Vec<f64> = (0..=120).map(|i| 400.0 + i as f64 * 2.5).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| {
            let a = (x - 520.0) / 38.0;
            let b = (x - 610.0) / 22.0;
            0.85 * (-a * a).exp() + 0.32 * (-b * b).exp() + 0.04
        })
        .collect();

    let x = project.add_column(sheet, None, Some("Lambda"), numerique(&xs))?;
    let y = project.add_column(sheet, None, Some("Absorbance"), numerique(&ys))?;

    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "λ".into(),
        unit: "nm".into(),
        grid: true,
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Absorbance mesurée".into(),
        unit: "u.a.".into(),
        grid: true,
        ..AxisSpec::default()
    };
    let mut serie = SeriesSpec::new_xy(x, y);
    serie.name = "Échantillon µ1".into();
    serie.line = LineSpec {
        color: "#4c78a8".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(serie);

    let spec = PlotSpec {
        title: "Spectre d'absorption — λ_max".into(),
        background: "#ffffff".into(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel);

    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    plotyx_render_svg::render_to_file(&cible, &plot, (900, 600), &measurer)?;

    println!("écrit : {cible} ({} points)", plot.point_count());
    Ok(())
}

fn numerique(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("colonne numérique valide")
}
