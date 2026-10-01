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

/// Issue #34 : un graphe à texte Typst s'exporte en SVG, PDF et PNG. Le PDF et
/// le PNG sont tirés du SVG par `usvg`, qui refusait le document tant que le
/// fragment Typst utilisait `xlink:href` sans déclarer le préfixe — le
/// défaut que ce test aurait vu. Et le texte de la figure n'est plus du
/// `<text>` brut : aucun `$` n'en reste, il est en contours Typst.
#[test]
fn un_graphe_typst_sexporte_en_svg_pdf_et_png() {
    use plotyx_render_svg::{export_bytes, ExportFormat};
    let project = Project::new("Typst export").expect("projet");
    let mut spec = spec_with_label("$lambda$ (nm)");
    spec.title = "Spectre $Delta H$".to_owned();
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let exporte = |format| {
        export_bytes(&plot, (480, 360), &measurer, &choice, format)
            .unwrap_or_else(|error| panic!("export {format:?} : {error}"))
    };

    let svg = String::from_utf8(exporte(ExportFormat::Svg)).expect("SVG en UTF-8");
    for (debut, _) in svg.match_indices("<text") {
        let fin = svg[debut..]
            .find("</text>")
            .map_or(svg.len(), |i| debut + i);
        assert!(
            !svg[debut..fin].contains('$'),
            "texte brut à dollars dans l'export : {}",
            &svg[debut..fin]
        );
    }
    assert!(svg.contains("xmlns:xlink"), "préfixe xlink non déclaré");

    let pdf = exporte(ExportFormat::Pdf);
    assert!(pdf.starts_with(b"%PDF"), "pas un PDF");
    // Le témoin : le même graphe sans Typst donne un autre PDF.
    let brut = plotyx_render_svg::render_to_pdf(&plot, (480, 360), &measurer).expect("PDF brut");
    assert_ne!(pdf, brut, "le PDF ne doit rien à Typst");

    let png = exporte(ExportFormat::Png { scale: 2.0 });
    assert!(png.starts_with(b"\x89PNG"), "pas un PNG");
    // Largeur dans l'en-tête IHDR (octets 16 à 19, gros-boutiste).
    let largeur = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    assert_eq!(largeur, 960, "PNG pas à l'échelle 2");
}
