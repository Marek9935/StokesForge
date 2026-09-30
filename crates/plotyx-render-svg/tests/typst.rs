//! Chemin export de Typst — tâche R8.4, et le pendant export de
//! l'invariant 8.5.
//!
//! Compilé en permanence depuis l'issue #29 (plus de feature `typst`),
//! comme `plotyx-render-egui/tests/typst.rs`.

use plotyx_core::resolve::resolve_spec;
use plotyx_core::Project;
use plotyx_plot::{PanelSpec, Panels, PlotSpec};
use plotyx_render::{FontChoice, TextMeasurer};
use plotyx_render_svg::{render_to_string, render_to_string_with_typst};

fn spec_with_label(label: &str) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.left.label = label.to_owned();
    panel.bottom.label = "t (s)".to_owned();
    PlotSpec::empty("").with_panels(Panels::new(vec![panel]))
}

/// **Le pendant export du test le plus important de la phase** : le
/// document produit avec Typst activé contient un fragment `typst-svg`
/// inliné pour le label formule, et plus aucun témoin — la preuve que
/// `splice` a fait son travail, pas seulement que `estimate_text_size` a
/// changé de valeur.
#[test]
fn le_document_typst_contient_un_fragment_inline_sans_temoin_residuel() {
    let project = Project::new("Typst export").expect("projet");
    let spec = spec_with_label("$lambda$ (nm)");
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();

    let sans_typst =
        render_to_string(&plot, (400, 300), &measurer).expect("échec de l'export sans Typst");
    let avec_typst = render_to_string_with_typst(&plot, (400, 300), &measurer, &choice)
        .expect("échec de l'export avec Typst");

    // Le document reste un SVG valide et non vide, dans les deux cas.
    assert!(sans_typst.trim_start().starts_with("<svg"));
    assert!(avec_typst.trim_start().starts_with("<svg"));

    // Le fragment Typst inliné est présent : un `<svg>` **imbriqué**, en
    // plus de la balise racine que `SVGBackend` écrit de toute façon dans
    // les deux documents (d'où la comparaison par compte, pas par simple
    // présence).
    let comptage = |document: &str| document.matches("<svg").count();
    assert_eq!(comptage(&sans_typst), 1, "document sans Typst inattendu");
    assert!(
        comptage(&avec_typst) >= 2,
        "aucun <svg> imbriqué trouvé dans le document Typst :\n{avec_typst}"
    );

    // Aucun témoin ne subsiste dans le document final — `splice` a
    // remplacé tous les rectangles-témoins qu'il a posés.
    assert!(
        !avec_typst.contains("fill=\"#010203\""),
        "un témoin Typst n'a pas été substitué :\n{avec_typst}"
    );

    // Les deux documents ne sont pas identiques : la géométrie tracée
    // diffère (largeur de l'aire de graduation de gauche, mesurée par
    // Typst plutôt que par l'heuristique `epaint`).
    assert_ne!(sans_typst, avec_typst);
}

/// Le chemin sans Typst n'est pas affecté par l'existence de la feature :
/// [`render_to_string`] rend le même résultat qu'un appel qui ignorerait
/// tout label formule (aucune régression du chemin par défaut).
#[test]
fn render_to_string_ignore_la_syntaxe_formule_sans_lappel_typst() {
    let project = Project::new("Typst export").expect("projet");
    let spec = spec_with_label("$lambda$ (nm)");
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();

    let document = render_to_string(&plot, (400, 300), &measurer).expect("échec de l'export");
    assert!(
        document.contains("$lambda$"),
        "le texte source doit rester littéral"
    );
    assert_eq!(
        document.matches("<svg").count(),
        1,
        "aucun <svg> imbriqué ne doit apparaître sans l'appel Typst"
    );
}
