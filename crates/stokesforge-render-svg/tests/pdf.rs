//! Tâche R6.2 : le PDF s'ouvre, et ses dimensions correspondent à celles du
//! SVG dont il est issu.
//!
//! « S'ouvre » veut dire : un analyseur PDF indépendant du nôtre — `lopdf`,
//! qui ne partage aucun code avec `svg2pdf` — reparse le document sans
//! erreur. Une simple recherche d'octets dans notre propre sortie ne
//! prouverait rien de plus que « `svg2pdf` a écrit les octets que nous lui
//! avons demandé d'écrire » ; `lopdf` prouve que la structure PDF elle-même
//! (table d'objets, dictionnaire de page, `MediaBox`) est valide.
//!
//! « Ses dimensions correspondent » est vérifié à trois sources, pas une
//! seule : la taille demandée à l'export, la taille réellement écrite dans
//! l'attribut `width`/`height` du SVG, et la `MediaBox` que `lopdf` retrouve
//! dans le PDF. Comparer seulement aux deux premières ne verrait pas une
//! divergence entre [`render_to_string`] et [`render_to_pdf`] — deux appels
//! séparés au pipeline, même s'ils partagent aujourd'hui tout leur code.

use stokesforge_core::resolve::resolve_spec;
use stokesforge_render::sample;
use stokesforge_render::TextMeasurer;
use stokesforge_render_svg::{render_to_pdf, render_to_pdf_file, render_to_string};

/// Extrait la valeur numérique d'un attribut `nom="valeur"` d'un document
/// SVG. Panique si absent : dans ce fichier, son absence est un échec du
/// test, pas un cas à tolérer.
fn attribut_numerique(document: &str, nom: &str) -> f64 {
    let motif = format!("{nom}=\"");
    let debut = document
        .find(&motif)
        .unwrap_or_else(|| panic!("attribut `{nom}` absent du SVG"))
        + motif.len();
    let reste = &document[debut..];
    let fin = reste
        .find('"')
        .unwrap_or_else(|| panic!("attribut `{nom}` non refermé"));
    reste[..fin]
        .parse()
        .unwrap_or_else(|err| panic!("attribut `{nom}` non numérique : {err}"))
}

/// La `MediaBox` de la première page d'un PDF, `(largeur, hauteur)` en
/// points — lue par `lopdf`, indépendamment de `svg2pdf`.
fn media_box(pdf: &[u8]) -> (f64, f64) {
    let document = lopdf::Document::load_mem(pdf).expect("le PDF doit s'ouvrir sans erreur");
    let pages = document.get_pages();
    assert_eq!(pages.len(), 1, "une figure exportée = une page PDF");
    let (_, id) = pages.into_iter().next().expect("au moins une page");
    let page = document
        .get_dictionary(id)
        .expect("dictionnaire de page introuvable");
    let boite = page
        .get(b"MediaBox")
        .expect("MediaBox absente de la page")
        .as_array()
        .expect("MediaBox n'est pas un tableau");
    let valeur = |i: usize| {
        boite[i]
            .as_float()
            .unwrap_or_else(|_| panic!("MediaBox[{i}] n'est pas numérique")) as f64
    };
    (valeur(2) - valeur(0), valeur(3) - valeur(1))
}

#[test]
fn le_pdf_souvre_et_ses_dimensions_correspondent_au_svg() {
    // Le graphe réaliste de R5 : cinq panneaux, six types de tracé, échelle
    // logarithmique — pas un canevas vide qui s'accorderait trivialement.
    let echantillon = sample::realistic().expect("graphe réaliste");
    let plot = resolve_spec(echantillon.project.tree(), &echantillon.spec);
    let measurer = TextMeasurer::default();
    let taille = (1280, 860);

    let document = render_to_string(&plot, taille, &measurer).expect("export SVG");
    let pdf = render_to_pdf(&plot, taille, &measurer).expect("export PDF");

    assert!(
        pdf.len() > 2_000,
        "PDF suspicieusement court ({} octets) pour cinq panneaux",
        pdf.len()
    );

    let (largeur_svg, hauteur_svg) = (
        attribut_numerique(&document, "width"),
        attribut_numerique(&document, "height"),
    );
    let (largeur_pdf, hauteur_pdf) = media_box(&pdf);

    // Trois sources, un seul nombre attendu de chaque côté : la taille
    // demandée, celle réellement écrite dans le SVG, et celle que `lopdf`
    // retrouve dans le PDF — à `PageOptions::default()` (72 DPI), les unités
    // utilisateur du SVG et les points PDF coïncident un pour un.
    assert_eq!(
        largeur_svg, taille.0 as f64,
        "largeur SVG ≠ largeur demandée"
    );
    assert_eq!(
        hauteur_svg, taille.1 as f64,
        "hauteur SVG ≠ hauteur demandée"
    );
    assert_eq!(
        largeur_pdf, largeur_svg,
        "largeur PDF ({largeur_pdf}) ≠ largeur SVG ({largeur_svg})"
    );
    assert_eq!(
        hauteur_pdf, hauteur_svg,
        "hauteur PDF ({hauteur_pdf}) ≠ hauteur SVG ({hauteur_svg})"
    );
}

#[test]
fn une_autre_taille_de_figure_donne_une_autre_mediabox() {
    // Témoin : si `media_box` renvoyait toujours la même chose (par exemple
    // parce que `render_to_pdf` ignorait `taille`), le test précédent
    // passerait pour une mauvaise raison. Le graphe minimal suffit ici.
    let echantillon = sample::minimal().expect("graphe minimal");
    let plot = resolve_spec(echantillon.project.tree(), &echantillon.spec);
    let measurer = TextMeasurer::default();

    let petit = render_to_pdf(&plot, (400, 300), &measurer).expect("export PDF (400×300)");
    let grand = render_to_pdf(&plot, (900, 600), &measurer).expect("export PDF (900×600)");

    assert_eq!(media_box(&petit), (400.0, 300.0));
    assert_eq!(media_box(&grand), (900.0, 600.0));
    assert_ne!(media_box(&petit), media_box(&grand));
}

#[test]
fn ecriture_pdf_sur_disque() {
    let echantillon = sample::minimal().expect("graphe minimal");
    let plot = resolve_spec(echantillon.project.tree(), &echantillon.spec);
    let dossier = tempfile::tempdir().expect("dossier temporaire");
    let cible = dossier.path().join("figure.pdf");

    render_to_pdf_file(&cible, &plot, (640, 480), &TextMeasurer::default()).expect("écriture");

    let contenu = std::fs::read(&cible).expect("relecture");
    assert!(contenu.starts_with(b"%PDF-"), "en-tête PDF absent");
    assert_eq!(media_box(&contenu), (640.0, 480.0));
}
