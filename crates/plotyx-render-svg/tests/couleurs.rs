//! Couleurs du texte et des échelles, vues dans le document exporté : la
//! même règle que l'écran, puisque le pipeline est commun.

use plotyx_core::resolve::resolve_spec;
use plotyx_core::Project;
use plotyx_plot::{PanelSpec, Panels, PlotSpec};
use plotyx_render::TextMeasurer;
use plotyx_render_svg::render_to_string;

fn exporte(spec: &PlotSpec) -> String {
    let project = Project::new("Couleurs").expect("projet");
    let plot = resolve_spec(project.tree(), spec);
    render_to_string(&plot, (400, 300), &TextMeasurer::default()).expect("export")
}

fn graphe(fond: &str) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom.label = "Temps".to_owned();
    panel.left.label = "Signal".to_owned();
    PlotSpec {
        background: fond.to_owned(),
        ..PlotSpec::empty("Titre")
    }
    .with_panels(Panels::new(vec![panel]))
}

/// Sur un fond sombre, tout ce qui est en couleur automatique passe en
/// blanc : titre, titres d'axes, graduations. Sur un fond clair, rien n'est
/// blanc.
#[test]
fn un_fond_sombre_passe_le_texte_automatique_en_blanc() {
    let sombre = exporte(&graphe("#101820")).to_uppercase();
    assert!(
        sombre.contains("#FFFFFF"),
        "texte blanc attendu sur fond sombre"
    );
    assert!(
        !sombre.contains("#202020"),
        "plus d'encre sombre sur fond sombre"
    );

    let clair = exporte(&graphe("#ffffff")).to_uppercase();
    assert!(clair.contains("#202020"), "encre sombre sur fond clair");
}

/// Une couleur explicite prime sur l'encre automatique.
#[test]
fn une_couleur_explicite_prime_sur_l_automatique() {
    let mut spec = graphe("#ffffff");
    spec.title_color = "#aa3311".to_owned();
    if let Some(panel) = spec.panels.get_mut(0) {
        panel.bottom.title_color = "#1177cc".to_owned();
        panel.left.tick_color = "#22aa44".to_owned();
    }
    let document = exporte(&spec).to_uppercase();
    for couleur in ["#AA3311", "#1177CC", "#22AA44"] {
        assert!(document.contains(couleur), "{couleur} absente du document");
    }
}
