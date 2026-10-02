//! Objets graphiques (issue #76) vus dans le document exporté : le pipeline
//! est commun à l'écran et à l'export, donc ce qu'on pose se retrouve dans le
//! SVG, avec ses couleurs.

use plotyx_core::resolve::resolve_spec;
use plotyx_core::Project;
use plotyx_plot::{PanelSpec, Panels, PlotSpec, ShapeKind, ShapeSpec};
use plotyx_render::TextMeasurer;
use plotyx_render_svg::render_to_string;

fn exporte(shapes: Vec<ShapeSpec>, fond: &str) -> String {
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom.auto_range = false;
    panel.bottom.range_min = Some(0.0);
    panel.bottom.range_max = Some(10.0);
    panel.left.auto_range = false;
    panel.left.range_min = Some(0.0);
    panel.left.range_max = Some(10.0);
    panel.shapes = shapes;
    let spec = PlotSpec {
        background: fond.to_owned(),
        ..PlotSpec::empty("Objets")
    }
    .with_panels(Panels::new(vec![panel]));
    let project = Project::new("Objets").expect("projet");
    let plot = resolve_spec(project.tree(), &spec);
    render_to_string(&plot, (400, 300), &TextMeasurer::default()).expect("export")
}

/// Trait et remplissage d'un rectangle, trait d'une flèche : leurs couleurs
/// sont dans le document ; sans objet, elles n'y sont pas (témoin).
#[test]
fn les_objets_sont_traces_dans_l_export_avec_leurs_couleurs() {
    let objets = vec![
        ShapeSpec {
            line_color: "#aa3311".to_owned(),
            fill_color: "#336699".to_owned(),
            ..ShapeSpec::new(ShapeKind::Rectangle, 2.0, 2.0, 6.0, 5.0)
        },
        ShapeSpec {
            line_color: "#11aa33".to_owned(),
            ..ShapeSpec::new(ShapeKind::Arrow, 1.0, 8.0, 8.0, 9.0)
        },
        ShapeSpec::new(ShapeKind::Ellipse, 5.0, 5.0, 9.0, 8.0),
    ];
    let avec = exporte(objets, "#ffffff").to_uppercase();
    for couleur in ["#AA3311", "#336699", "#11AA33"] {
        assert!(avec.contains(couleur), "{couleur} absente du document");
    }
    let sans = exporte(Vec::new(), "#ffffff").to_uppercase();
    for couleur in ["#AA3311", "#336699", "#11AA33"] {
        assert!(!sans.contains(couleur), "{couleur} sans objet : témoin");
    }
}

/// Un trait sans couleur prend l'encre automatique : blanc sur fond sombre.
#[test]
fn un_trait_automatique_suit_l_encre_du_fond() {
    let ligne = || vec![ShapeSpec::new(ShapeKind::Line, 1.0, 1.0, 9.0, 9.0)];
    let sombre = exporte(ligne(), "#101820");
    let clair = exporte(ligne(), "#ffffff");
    // Le nombre de tracés blancs augmente avec la ligne sur fond sombre.
    let sans = exporte(Vec::new(), "#101820");
    let blancs = |s: &str| s.to_uppercase().matches("#FFFFFF").count();
    assert!(blancs(&sombre) > blancs(&sans));
    let sans_clair = exporte(Vec::new(), "#ffffff");
    let encre = |s: &str| s.to_uppercase().matches("#202020").count();
    assert!(encre(&clair) > encre(&sans_clair));
}
