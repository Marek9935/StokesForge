//! Chemin export de Typst — tâche R8.4, et le pendant export de
//! l'invariant 8.5.
//!
//! Compilé en permanence depuis l'issue #29 (plus de feature `typst`),
//! comme `stokesforge-render-egui/tests/typst.rs`.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_core::Project;
use stokesforge_plot::{PanelSpec, Panels, PlotSpec};
use stokesforge_render::{FontChoice, TextMeasurer};
use stokesforge_render_svg::{render_to_string, render_to_string_with_typst};

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
    let avec_typst = render_to_string_with_typst(
        &plot,
        (400, 300),
        &measurer,
        &choice,
        stokesforge_render_svg::SvgTextMode::Outlines,
    )
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
    use stokesforge_render_svg::{export_bytes, ExportFormat};
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

    let svg = String::from_utf8(exporte(ExportFormat::Svg {
        text: stokesforge_render_svg::SvgTextMode::Text,
    }))
    .expect("SVG en UTF-8");
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
    let brut =
        stokesforge_render_svg::render_to_pdf(&plot, (480, 360), &measurer).expect("PDF brut");
    assert_ne!(pdf, brut, "le PDF ne doit rien à Typst");

    let png = exporte(ExportFormat::Png { scale: 2.0 });
    assert!(png.starts_with(b"\x89PNG"), "pas un PNG");
    // Largeur dans l'en-tête IHDR (octets 16 à 19, gros-boutiste).
    let largeur = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    assert_eq!(largeur, 960, "PNG pas à l'échelle 2");
}

/// #149 : l'export groupé donne, format par format, les mêmes octets que
/// l'export isolé — il ne fait qu'éviter de composer trois fois la figure.
/// Témoins : l'ordre demandé est l'ordre rendu (un SVG n'est pas un PNG), et
/// une liste vide ne rend rien.
#[test]
fn l_export_groupe_rend_les_memes_octets_que_l_export_isole() {
    use stokesforge_render_svg::{export_bytes, export_bytes_many, ExportFormat};
    let project = Project::new("Export groupé").expect("projet");
    let mut spec = spec_with_label("$lambda$ (nm)");
    spec.title = "Spectre $Delta H$".to_owned();
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let formats = [
        ExportFormat::Png { scale: 1.0 },
        ExportFormat::Svg {
            text: stokesforge_render_svg::SvgTextMode::Text,
        },
        ExportFormat::Pdf,
    ];
    let groupe = export_bytes_many(&plot, (480, 360), &measurer, &choice, &formats)
        .expect("tracé")
        .into_iter()
        .map(|octets| octets.expect("conversion"))
        .collect::<Vec<_>>();
    assert_eq!(groupe.len(), 3);
    assert!(groupe[0].starts_with(b"\x89PNG"), "premier : PNG");
    assert!(groupe[1].starts_with(b"<?xml") || groupe[1].starts_with(b"<svg"));
    assert!(groupe[2].starts_with(b"%PDF"), "troisième : PDF");
    for (format, octets) in formats.iter().zip(&groupe) {
        let isole = export_bytes(&plot, (480, 360), &measurer, &choice, *format).expect("export");
        if matches!(format, ExportFormat::Pdf) {
            // `svg2pdf` ne numérote pas les polices de façon reproductible
            // dès que le PDF porte du texte (#162) : deux exports du même
            // SVG diffèrent par des octets d'identifiant, pas de contenu.
            assert_eq!(isole.len(), octets.len(), "Pdf : longueur différente");
        } else {
            assert_eq!(&isole, octets, "{format:?} diffère de l'export isolé");
        }
    }
    let vide = export_bytes_many(&plot, (480, 360), &measurer, &choice, &[]).expect("tracé");
    assert!(vide.is_empty());
}

/// Issues #77 et #78 : la police et le corps réglés sur une zone de texte
/// changent l'export — le titre d'axe composé en Fira Sans 28 n'est pas celui
/// composé dans la police du graphe, et les autres zones ne bougent pas de
/// police.
#[test]
fn la_police_d_une_zone_de_texte_change_l_export() {
    let project = Project::new("Police par zone").expect("projet");
    let mut spec = spec_with_label("Absorbance");
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let export = |plot: &stokesforge_core::resolve::ResolvedPlot<'_>| -> String {
        render_to_string_with_typst(
            plot,
            (500, 400),
            &measurer,
            &choice,
            stokesforge_render_svg::SvgTextMode::Outlines,
        )
        .expect("export")
    };
    let defaut = export(&plot);

    let font = stokesforge_plot::TextFont {
        family: "Fira Sans".to_owned(),
        size: Some(28.0),
    };
    spec.title_font = font.clone();
    spec.title = "Titre".to_owned();
    let mut panels = spec.panels.iter().cloned().collect::<Vec<_>>();
    panels[0].left.title_font = font;
    let spec = spec.with_panels(Panels::new(panels));
    let plot = resolve_spec(project.tree(), &spec);
    let regle = export(&plot);
    assert_ne!(defaut, regle, "la police et le corps n'ont rien changé");
    if let Ok(path) = std::env::var("STOKESFORGE_TEST_SVG_OUT") {
        std::fs::write(path, &regle).expect("écriture de l'export");
    }
}

// ── #162 : le texte Typst reste du texte SVG ─────────────────────────────

/// Un graphe à titre, titres d'axe, légende, formule et fraction.
fn spec_riche() -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.left.label = "$lambda_\"max\"$ (nm)".to_owned();
    panel.bottom.label = "Temps $a/b$".to_owned();
    let mut spec = PlotSpec::empty("").with_panels(Panels::new(vec![panel]));
    spec.title = "Spectre alpha".to_owned();
    spec
}

fn svg_mode(spec: &PlotSpec, mode: stokesforge_render_svg::SvgTextMode) -> String {
    let project = Project::new("Texte").expect("projet");
    let plot = resolve_spec(project.tree(), spec);
    render_to_string_with_typst(
        &plot,
        (480, 360),
        &TextMeasurer::default(),
        &FontChoice::default(),
        mode,
    )
    .expect("export")
}

/// Les contenus des `<text>` d'un document, concaténés.
fn textes(svg: &str) -> String {
    let mut out = String::new();
    for (debut, _) in svg.match_indices("<text") {
        let Some(open_end) = svg[debut..].find('>') else {
            continue;
        };
        let fin = svg[debut..]
            .find("</text>")
            .map_or(svg.len(), |i| debut + i);
        out.push_str(&svg[debut + open_end + 1..fin]);
        out.push('\n');
    }
    out
}

#[test]
fn le_texte_typst_sort_en_text_svg_et_la_geometrie_ne_bouge_pas() {
    use stokesforge_render_svg::SvgTextMode::{Outlines, Text};
    let spec = spec_riche();
    let texte = svg_mode(&spec, Text);
    let contours = svg_mode(&spec, Outlines);

    let contenu = textes(&texte);
    for attendu in ["Spectre", "alpha", "Temps", "nm"] {
        assert!(
            contenu.contains(attendu) || contenu.contains(&attendu.chars().collect::<String>()),
            "{attendu:?} absent des <text> :\n{contenu}"
        );
    }
    // La formule `lambda` sort comme le caractère Unicode (ou, à défaut,
    // en contour), jamais comme la source `lambda`.
    assert!(
        !contenu.contains("lambda"),
        "source Typst brute : {contenu}"
    );
    // Une famille nommée avec repli générique, jamais d'`@font-face`.
    assert!(texte.contains("font-family=\"'Noto Sans', sans-serif\""));
    assert!(!texte.contains("@font-face"));

    // Le mode Contours n'écrit aucun texte, graduations comprises.
    assert!(
        !contours.contains("<text"),
        "un <text> subsiste en contours :\n{contours}"
    );
    // Même fenêtre, même cadre : la figure ne bouge pas d'un mode à l'autre.
    let cadre = |svg: &str| svg.lines().next().map(str::to_owned);
    assert_eq!(cadre(&texte), cadre(&contours));
}

/// Les graduations portent la famille du graphe, comme les textes Typst.
#[test]
fn les_graduations_portent_la_police_du_graphe() {
    use stokesforge_render::{FontChoice, FontSet};
    use stokesforge_render_svg::SvgTextMode::Text;
    let project = Project::new("Police").expect("projet");
    let spec = spec_riche();
    let plot = resolve_spec(project.tree(), &spec);
    for set in FontSet::ALL {
        let svg = render_to_string_with_typst(
            &plot,
            (480, 360),
            &TextMeasurer::with_font_choice(1.0, &FontChoice::Embedded(set)),
            &FontChoice::Embedded(set),
            Text,
        )
        .expect("export");
        assert!(
            !svg.contains("font-family=\"sans-serif\""),
            "graduation générique avec {set:?}"
        );
        // Plotters échappe les apostrophes de l'attribut.
        let famille = format!(
            "font-family=\"&apos;{}&apos;, sans-serif\"",
            set.sans_family_name()
        );
        assert!(svg.contains(&famille), "{famille} absent avec {set:?}");
    }
}

/// Un glyphe sans caractère Unicode (délimiteur étiré d'une grande
/// fraction) repasse en contour : un `<path>` avec transformation de glyphe.
#[test]
fn un_delimiteur_etire_repasse_en_contour() {
    use stokesforge_render_svg::SvgTextMode::Text;
    let mut spec = spec_riche();
    spec.title = "$ lr(( a/b )) $".to_owned();
    let svg = svg_mode(&spec, Text);
    assert!(
        svg.contains("<path transform=\"translate("),
        "aucun glyphe en contour pour un délimiteur étiré :\n{svg}"
    );
}

/// Le PDF tiré du mode texte contient du vrai texte, extractible.
#[test]
fn le_pdf_contient_du_texte_extractible() {
    use stokesforge_render_svg::{export_bytes, ExportFormat};
    let project = Project::new("PDF").expect("projet");
    let spec = spec_riche();
    let plot = resolve_spec(project.tree(), &spec);
    let pdf = export_bytes(
        &plot,
        (480, 360),
        &TextMeasurer::default(),
        &FontChoice::default(),
        ExportFormat::Pdf,
    )
    .expect("PDF");
    let document = lopdf::Document::load_mem(&pdf).expect("PDF relisible");
    let page = *document.get_pages().keys().next().expect("une page");
    let extrait: String = document
        .extract_text(&[page])
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    assert!(
        extrait.contains("Temps") && extrait.contains("Spectre"),
        "texte du PDF non extractible : {extrait:?}"
    );
}
