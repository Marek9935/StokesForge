//! Ce que le test d'accord ne peut pas voir : les `Shape::Text`.
//!
//! `tests/accord.rs` compare les segments — donc les axes, la grille et la
//! courbe — entre les deux backends. Il est aveugle au texte, pour une
//! raison de fond : côté SVG le texte est un élément `<text>` ancré, côté
//! egui c'est un `Galley` positionné et tourné. Les deux sont corrects et
//! incomparables.
//!
//! Or c'est exactement là que vit la classe de bug la plus bête de ce
//! backend : un ancrage mal traduit ou une rotation appliquée au mauvais
//! vecteur, et le titre de l'axe de gauche part hors du cadre — sans aucune
//! erreur, et sans que le test d'accord bronche. D'où ces vérifications, qui
//! remplacent le coup d'œil que la fenêtre donnerait.

use egui::{Pos2, Rect, Shape};
use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::resolve::resolve_spec;
use plotyx_core::Project;
use plotyx_plot::{AxisSpec, LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_render::TextMeasurer;
use plotyx_render_egui::collect_shapes;

const SIZE: (f32, f32) = (800.0, 600.0);
const POINTS: usize = 64;

fn figure() -> Vec<Shape> {
    let mut project = Project::new("Formes").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let xs: Vec<f64> = (0..POINTS).map(|i| i as f64).collect();
    let ys: Vec<f64> = xs.iter().map(|x| (x / 8.0).sin()).collect();
    let x = project
        .add_column(sheet, None, Some("X"), numeric(&xs))
        .expect("X");
    let y = project
        .add_column(sheet, None, Some("Y"), numeric(&ys))
        .expect("Y");

    let mut panel = PanelSpec::at(0, 0);
    panel.bottom = AxisSpec {
        label: "Temps".into(),
        unit: "s".into(),
        ..AxisSpec::default()
    };
    panel.left = AxisSpec {
        label: "Amplitude".into(),
        unit: "V".into(),
        ..AxisSpec::default()
    };
    let mut series = SeriesSpec::new_xy(x, y);
    series.line = LineSpec {
        color: "#4c78a8".into(),
        width: 2.0,
        ..LineSpec::default()
    };
    panel.series.push(series);
    let spec = PlotSpec {
        title: "Sinusoïde".into(),
        background: "#ffffff".into(),
        ..PlotSpec::empty("")
    }
    .with_panels(panel);

    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let mut shapes = Vec::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(SIZE.0, SIZE.1));
    collect_shapes(&mut shapes, rect, &measurer, &plot).expect("tracé");
    shapes
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("colonne")
}

#[test]
fn rien_ne_deborde_du_rectangle_alloue() {
    let cadre = Rect::from_min_size(Pos2::ZERO, egui::vec2(SIZE.0, SIZE.1));
    // Une marge d'un point : les traits d'axe sont posés sur la bordure et
    // leur épaisseur déborde d'un demi-pixel de chaque côté.
    let tolerance = 1.5;
    for shape in figure() {
        let bounds = shape.visual_bounding_rect();
        if !bounds.is_finite() {
            continue;
        }
        assert!(
            bounds.min.x >= cadre.min.x - tolerance
                && bounds.min.y >= cadre.min.y - tolerance
                && bounds.max.x <= cadre.max.x + tolerance
                && bounds.max.y <= cadre.max.y + tolerance,
            "{bounds:?} sort du cadre {cadre:?} — un ancrage ou une rotation \
             de `draw_text` est mal traduit, et ce texte serait invisible à \
             l'écran"
        );
    }
}

#[test]
fn le_titre_de_laxe_de_gauche_est_tourne_et_a_gauche() {
    let textes: Vec<_> = figure()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text),
            _ => None,
        })
        .collect();
    assert!(!textes.is_empty(), "aucun texte : les graduations manquent");

    let vertical = textes
        .iter()
        .find(|text| text.galley.job.text.contains("Amplitude"))
        .expect("le titre de l'axe de gauche doit être dessiné");
    assert!(
        (vertical.angle - 3.0 * std::f32::consts::FRAC_PI_2).abs() < 1e-4,
        "titre d'axe vertical à {} rad au lieu de 3π/2 : la conversion \
         FontTransform → TextShape::angle est fausse",
        vertical.angle
    );
    assert!(
        vertical.visual_bounding_rect().max.x < SIZE.0 * 0.15,
        "le titre de l'axe de gauche n'est pas à gauche : {:?}",
        vertical.visual_bounding_rect()
    );

    let titre = textes
        .iter()
        .find(|text| text.galley.job.text.contains("Sinusoïde"))
        .expect("le titre du graphe doit être dessiné");
    let boite = titre.visual_bounding_rect();
    assert!(
        titre.angle == 0.0 && boite.min.y < 40.0,
        "le titre du graphe devrait être droit et en haut : {boite:?}, angle {}",
        titre.angle
    );
    // Centré : `titled` ancre au centre horizontal, donc le décalage
    // d'ancrage doit avoir été appliqué.
    assert!(
        (boite.center().x - SIZE.0 / 2.0).abs() < 2.0,
        "titre non centré : {boite:?}"
    );
}

#[test]
fn la_serie_est_une_seule_polyligne() {
    // Ce n'est pas un détail de forme : toute l'économie du cache repose
    // dessus. Une `Shape::Path` par série coûte 64 octets plus ses points ;
    // N `LineSegment` — ce que produirait l'implémentation par défaut de
    // `draw_path` — en coûteraient 64 chacun, et la mesure R0 0.3 (2,432 Mo
    // pour 300 000 points) ne tiendrait plus.
    let bleu = egui::Color32::from_rgb(0x4c, 0x78, 0xa8);
    let series: Vec<_> = figure()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(bleu) => {
                Some(path)
            }
            _ => None,
        })
        .collect();
    // Deux depuis R5 : la courbe, et la vignette de la série dans la
    // légende — qui est peinte avec le même style, c'est le principe d'une
    // vignette. Ce que ce test verrouille est que la **courbe** reste une
    // seule forme, pas que la figure n'en porte qu'une.
    assert_eq!(
        series.len(),
        2,
        "attendu : la courbe et sa vignette de légende"
    );
    let courbe = series
        .iter()
        .max_by_key(|path| path.points.len())
        .expect("la courbe");
    assert_eq!(
        courbe.points.len(),
        POINTS,
        "la polyligne devrait porter tous les points de la colonne"
    );
    assert_eq!(
        courbe.stroke.width, 2.0,
        "l'épaisseur du spec n'est pas reportée"
    );
    let vignette = series
        .iter()
        .min_by_key(|path| path.points.len())
        .expect("la vignette");
    assert_eq!(
        vignette.points.len(),
        2,
        "la vignette de légende est un simple segment"
    );
}
