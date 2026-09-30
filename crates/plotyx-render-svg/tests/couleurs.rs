//! Couleurs du texte et des échelles, vues dans le document exporté : la
//! même règle que l'écran, puisque le pipeline est commun.

use plotyx_core::resolve::resolve_spec;
use plotyx_core::Project;
use plotyx_plot::{PanelSpec, Panels, PlotSpec};
use plotyx_render::{FontChoice, TextMeasurer};
use plotyx_render_svg::{render_to_string, render_to_string_with_typst};

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

/// Le fragment Typst du titre, tel que `splice` l'a inliné : du `<svg>`
/// imbriqué (le premier après la racine) jusqu'à la fin de son groupe.
/// `document` est passé en majuscules par l'appelant, pour comparer les
/// couleurs sans casse.
fn fragment_typst(document: &str) -> &str {
    let racine = document.find("<SVG").expect("document vide") + 1;
    let debut = racine
        + document[racine..]
            .find("<SVG WIDTH=")
            .expect("aucun fragment Typst");
    let fin = document[debut..]
        .find("</SVG></G>")
        .expect("fragment Typst non refermé");
    &document[debut..debut + fin]
}

fn exporte_avec_typst(spec: &PlotSpec) -> String {
    let project = Project::new("Couleurs").expect("projet");
    let plot = resolve_spec(project.tree(), spec);
    render_to_string_with_typst(
        &plot,
        (400, 300),
        &TextMeasurer::default(),
        &FontChoice::default(),
    )
    .expect("export")
}

/// Issue #30 : un titre Typst prend la couleur du texte. Sur un fond
/// sombre, l'encre automatique est blanche, et c'est le fragment Typst
/// lui-même qui la porte — avant, il était toujours noir. Le témoin est le
/// même titre sur fond clair : son fragment n'est pas blanc.
#[test]
fn un_titre_typst_prend_l_encre_automatique() {
    let titre = |fond: &str| PlotSpec {
        title: "$lambda$ (nm)".to_owned(),
        ..graphe(fond)
    };
    let sombre = exporte_avec_typst(&titre("#101820")).to_uppercase();
    let fragment = fragment_typst(&sombre);
    assert!(
        fragment.contains("#FFFFFF"),
        "titre Typst pas blanc :\n{fragment}"
    );
    assert!(!fragment.contains("#000000"), "titre Typst encore noir");

    let clair = exporte_avec_typst(&titre("#ffffff")).to_uppercase();
    let fragment = fragment_typst(&clair);
    assert!(!fragment.contains("#FFFFFF"), "le témoin clair est blanc");
    assert!(fragment.contains("#202020"), "encre sombre attendue");
}

/// Une couleur de titre explicite passe dans le fragment Typst, et un titre
/// transparent ne dessine rien — ni texte, ni fragment.
#[test]
fn un_titre_typst_suit_title_color() {
    let mut spec = PlotSpec {
        title: "$Delta H$".to_owned(),
        ..graphe("#ffffff")
    };
    spec.title_color = "#aa3311".to_owned();
    let document = exporte_avec_typst(&spec).to_uppercase();
    assert!(fragment_typst(&document).contains("#AA3311"));

    spec.title_color = "#aa331100".to_owned();
    let document = exporte_avec_typst(&spec);
    assert_eq!(
        document.matches("<svg").count(),
        1,
        "un titre transparent a produit un fragment Typst"
    );
}
