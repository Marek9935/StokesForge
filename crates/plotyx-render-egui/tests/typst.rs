//! Chemin écran de Typst — tâche R8.3, et **le test de l'invariant 8.5**.
//!
//! Compilé en permanence depuis l'issue #29 (plus de feature `typst`) :
//! c'est désormais `cargo test --workspace` qui tient ce chemin.

use egui::{FontFamily, FontId, Pos2, Rect, Shape};
use plotyx_core::resolve::resolve_spec;
use plotyx_core::Project;
use plotyx_plot::{PanelSpec, Panels, PlotSpec};
use plotyx_render::formula;
use plotyx_render::formula::FormulaCache;
use plotyx_render::text::{font_size, round_size, TextSource};
use plotyx_render::{FontChoice, TextMeasurer};
use plotyx_render_egui::{collect_shapes_with_typst, ScreenText, TypstScreen};

fn spec_with_label(label: &str) -> PlotSpec {
    let mut panel = PanelSpec::at(0, 0);
    panel.left.label = label.to_owned();
    panel.bottom.label = "t (s)".to_owned();
    PlotSpec::empty("").with_panels(Panels::new(vec![panel]))
}

/// **Le test le plus important de la phase (consigne de la mission)** :
/// `estimate_text_size` doit interroger Typst pour un label Typst, pas
/// `epaint::Fonts` — sans quoi Plotters réserverait la marge d'un texte
/// `"$lambda$ (nm)"` de neuf caractères plutôt que celle d'une formule
/// rendue, et la figure se décalerait sans qu'aucune erreur ne le signale
/// (`architecture-choisie.md` §4.3).
///
/// Vérifié en comparant, à la même figure, le rendu produit avec et sans
/// le chemin Typst activé sur le backend : la géométrie tracée (donc les
/// formes produites) doit différer, et la formule doit apparaître comme un
/// `Shape::Mesh` texturé plutôt que comme le `Shape::Text` de sa chaîne
/// source littérale.
#[test]
fn estimate_text_size_interroge_typst_pour_un_label_formule() {
    let ctx = egui::Context::default();
    // Comme `plotyx-app` et le reste des tests de ce crate
    // (`cache_session.rs`) : la source unique de polices est posée
    // explicitement.
    ctx.set_fonts(plotyx_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();

    let project = Project::new("Typst").expect("projet");
    let spec = spec_with_label("$lambda$ (nm)");
    let plot = resolve_spec(project.tree(), &spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));

    // Sans Typst : la mesure `epaint` de la chaîne source telle quelle.
    // `load_texture`/`fonts_mut` n'existent qu'à l'intérieur d'une image
    // (`Context::run_ui`) — sans quoi `epaint` panique (« No fonts
    // available until first call to Context::run() »), comme
    // `cache_session.rs` le documente déjà pour ce même piège.
    let mut shapes_sans_typst = Vec::new();
    let mut panels_sans_typst = 0;
    let mut sortie = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text_source = ScreenText::new(&ctx, &measurer);
        let drawn =
            plotyx_render_egui::collect_shapes(&mut shapes_sans_typst, rect, &text_source, &plot)
                .expect("échec du tracé sans Typst");
        panels_sans_typst = drawn.panels.len();
    });
    sortie.textures_delta.clear();

    // Avec Typst activé sur le backend.
    let mut shapes_avec_typst = Vec::new();
    let mut panels_avec_typst = 0;
    let mut sortie = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text_source = ScreenText::new(&ctx, &measurer);
        let typst =
            TypstScreen::new(&ctx, &cache, &choice).expect("résolution des polices échouée");
        let drawn =
            collect_shapes_with_typst(&mut shapes_avec_typst, rect, &text_source, typst, &plot)
                .expect("échec du tracé avec Typst");
        panels_avec_typst = drawn.panels.len();
    });
    sortie.textures_delta.clear();

    // La figure a bien été tracée (pas de dégradation silencieuse vers
    // « rien » : au moins un panneau, avec sa zone de graduation), des deux
    // côtés.
    assert_eq!(panels_sans_typst, 1);
    assert_eq!(panels_avec_typst, 1);

    // Le maillage texturé de la formule est bien présent parmi les formes
    // produites — la preuve directe que le chemin Typst a été emprunté,
    // pas seulement que la mesure a changé.
    let contient_un_maillage = shapes_avec_typst
        .iter()
        .any(|shape| matches!(shape, Shape::Mesh(_)));
    assert!(
        contient_un_maillage,
        "aucun Shape::Mesh trouvé : le label Typst n'a pas emprunté le chemin de rendu attendu"
    );

    // Les deux figures ne sont **pas** identiques : la présence du chemin
    // Typst change la géométrie tracée (largeur de l'aire de graduation de
    // gauche, mesurée par Typst plutôt que par `epaint`), donc au moins une
    // forme diffère entre les deux rendus.
    assert!(
        !shapes_sans_typst.is_empty(),
        "le rendu de référence est vide"
    );
    assert_ne!(
        format!("{shapes_sans_typst:?}"),
        format!("{shapes_avec_typst:?}"),
        "le rendu ne change pas selon que Typst est activé ou non pour ce label"
    );
}

/// La mesure `plotyx_render::formula`, isolée de tout backend : une
/// formule ne mesure pas la même chose que son texte source littéral vu par
/// `epaint` — condition nécessaire pour que le test ci-dessus distingue un
/// vrai branchement Typst d'un bug qui mesurerait toujours la chaîne
/// source.
#[test]
fn la_mesure_typst_differe_de_la_mesure_epaint_du_texte_source_litteral() {
    let (sans, math) = formula::resolve_fonts(&FontChoice::default()).unwrap();
    let compiled = formula::compile("$lambda$ (nm)", 14.0, [0, 0, 0, 255], sans, math).unwrap();
    let mesure_typst = formula::measure(&compiled);

    let mesurer = TextMeasurer::default();
    let font_id = FontId::new(font_size(14.0), FontFamily::Proportional);
    let mesure_epaint_du_source = round_size(mesurer.measure("$lambda$ (nm)", font_id));

    assert_ne!(
        mesure_typst, mesure_epaint_du_source,
        "la formule et son texte source brut mesurent la même chose : le \
         test ne distinguerait pas un vrai branchement Typst d'un bug qui \
         mesurerait toujours la chaîne source"
    );
}

/// Les pixels presque opaques (alpha ≥ 200 : à 1 pixel par point, un trait
/// fin n'atteint pas toujours 255) de la texture d'une formule, lus dans le
/// delta de textures de l'image qui l'a chargée. Couleurs **prémultipliées**,
/// comme `ColorImage` les garde.
fn pixels_de_la_formule(fond: &str) -> Vec<egui::Color32> {
    let ctx = egui::Context::default();
    ctx.set_fonts(plotyx_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();
    let project = Project::new("Typst").expect("projet");
    let spec = PlotSpec {
        background: fond.to_owned(),
        ..PlotSpec::empty("$Delta H$")
    }
    .with_panels(Panels::new(vec![PanelSpec::at(0, 0)]));
    let plot = resolve_spec(project.tree(), &spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));

    let mut shapes = Vec::new();
    let mut sortie = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text_source = ScreenText::new(&ctx, &measurer);
        let typst = TypstScreen::new(&ctx, &cache, &choice).expect("polices");
        collect_shapes_with_typst(&mut shapes, rect, &text_source, typst, &plot).expect("tracé");
    });
    let id = shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Mesh(mesh) => Some(mesh.texture_id),
            _ => None,
        })
        .expect("le titre Typst n'a pas produit de maillage");
    let (_, deltas) = sortie
        .textures_delta
        .set
        .iter()
        .find(|(texture, _)| **texture == id)
        .expect("texture du titre absente du delta");
    let egui::ImageData::Color(image) = &deltas[0].image;
    let opaques = image
        .pixels
        .iter()
        .copied()
        .filter(|pixel| pixel.a() >= 200)
        .collect();
    sortie.textures_delta.clear();
    opaques
}

/// Issue #30 : la texture d'un titre Typst porte la couleur du texte. Sur
/// un fond sombre, l'encre automatique est blanche ; le témoin, sur fond
/// clair, reste sombre. Avant, les deux étaient noirs.
#[test]
fn la_texture_dune_formule_prend_la_couleur_du_texte() {
    let sombre = pixels_de_la_formule("#101820");
    assert!(!sombre.is_empty(), "aucun pixel opaque");
    // Blanc prémultiplié : chaque composante vaut l'opacité.
    assert!(
        sombre
            .iter()
            .all(|pixel| pixel.r().abs_diff(pixel.a()) <= 3 && pixel.b().abs_diff(pixel.a()) <= 3),
        "titre Typst pas blanc sur fond sombre"
    );
    let clair = pixels_de_la_formule("#ffffff");
    assert!(!clair.is_empty(), "aucun pixel opaque (témoin)");
    assert!(
        clair.iter().all(|pixel| pixel.r() < 64),
        "le témoin sur fond clair n'est pas sombre"
    );
}

// -- Issue #32 : tout titre, titre d'axe et légende passe par Typst --

/// Un projet à deux séries nommées, de quoi remplir une légende.
fn projet_a_legende(noms: [&str; 2]) -> (Project, PlotSpec) {
    projet_a_colonnes(["Y0", "Y1"], noms)
}

/// Comme [`projet_a_legende`], en nommant aussi les colonnes Y ; un nom de
/// série vide laisse la légende reprendre celui de la colonne.
fn projet_a_colonnes(colonnes: [&str; 2], noms: [&str; 2]) -> (Project, PlotSpec) {
    use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
    use plotyx_plot::{LineSpec, SeriesSpec};
    let colonne = |values: Vec<f64>| {
        Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values),
            ColumnMeta::default(),
        )
        .expect("colonne")
    };
    let mut project = Project::new("Typst").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let xs: Vec<f64> = (0..20).map(f64::from).collect();
    let x = project
        .add_column(sheet, None, Some("X"), colonne(xs.clone()))
        .expect("X");
    let mut panel = PanelSpec::at(0, 0);
    panel.bottom.label = "$lambda$".to_owned();
    panel.bottom.unit = "nm".to_owned();
    panel.left.label = "Absorbance".to_owned();
    for (index, (colonne_y, nom)) in colonnes.into_iter().zip(noms).enumerate() {
        let ys: Vec<f64> = xs.iter().map(|x| x * (index + 1) as f64).collect();
        let y = project
            .add_column(sheet, None, Some(colonne_y), colonne(ys))
            .expect("Y");
        let mut series = SeriesSpec::new_xy(x, y);
        series.line = LineSpec::from_palette(index);
        series.name = nom.to_owned();
        panel.series.push(series);
    }
    let spec = PlotSpec::empty("Spectre").with_panels(Panels::new(vec![panel]));
    (project, spec)
}

/// Trace `spec` à l'écran, Typst activé ou non.
fn ecran(
    project: &Project,
    spec: &PlotSpec,
    avec_typst: bool,
) -> (Vec<Shape>, plotyx_render_egui::DrawnPlot) {
    let ctx = egui::Context::default();
    ctx.set_fonts(plotyx_render::font_definitions());
    let measurer = TextMeasurer::default();
    let choice = FontChoice::default();
    let cache: FormulaCache<_> = FormulaCache::new();
    let plot = resolve_spec(project.tree(), spec);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(480.0, 360.0));
    let mut shapes = Vec::new();
    let mut drawn = None;
    let mut sortie = ctx.run_ui(egui::RawInput::default(), |_ui| {
        let text = ScreenText::new(&ctx, &measurer);
        drawn = Some(if avec_typst {
            let typst = TypstScreen::new(&ctx, &cache, &choice).expect("polices");
            collect_shapes_with_typst(&mut shapes, rect, &text, typst, &plot).expect("tracé")
        } else {
            plotyx_render_egui::collect_shapes(&mut shapes, rect, &text, &plot).expect("tracé")
        });
    });
    sortie.textures_delta.clear();
    (shapes, drawn.expect("figure"))
}

/// Le coin haut-gauche non tourné de chaque maillage Typst — son premier
/// sommet (`typst_screen::mesh`) —, au demi-pixel, trié.
fn origines_ecran(shapes: &[Shape]) -> Vec<(i64, i64)> {
    let mut origines: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => mesh.vertices.first().map(|v| v.pos),
            _ => None,
        })
        .map(|pos| ((pos.x * 2.0).round() as i64, (pos.y * 2.0).round() as i64))
        .collect();
    origines.sort_unstable();
    origines
}

/// Le même relevé dans le SVG exporté : l'origine de chaque groupe Typst
/// (`translate(ox, oy)`, `typst_export::group_for`).
fn origines_export(document: &str) -> Vec<(i64, i64)> {
    let mut origines: Vec<_> = document
        .match_indices(") translate(")
        .filter_map(|(index, motif)| {
            let reste = &document[index + motif.len()..];
            let (x, reste) = reste.split_once(", ")?;
            let (y, _) = reste.split_once(')')?;
            let (x, y): (f64, f64) = (x.parse().ok()?, y.parse().ok()?);
            Some(((x * 2.0).round() as i64, (y * 2.0).round() as i64))
        })
        .collect();
    origines.sort_unstable();
    origines
}

/// Un titre **sans `$`** se mesure par Typst : la bande du titre n'a pas la
/// même hauteur que la mesure `epaint` du même texte (le témoin), et le titre
/// n'est plus un `Shape::Text`. Les graduations, elles, restent du texte
/// `epaint`.
#[test]
fn un_titre_sans_dollar_se_mesure_et_se_dessine_par_typst() {
    let (project, spec) = projet_a_legende(["A", "B"]);
    let (avec, figure_typst) = ecran(&project, &spec, true);
    let (_, figure_epaint) = ecran(&project, &spec, false);
    assert_ne!(
        figure_typst.title_height, figure_epaint.title_height,
        "le titre « Spectre » se mesure encore par epaint"
    );
    let textes: Vec<String> = avec
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    assert!(!textes.is_empty(), "plus aucune graduation epaint");
    for texte in textes {
        assert!(
            texte.parse::<f64>().is_ok(),
            "{texte:?} est resté en texte epaint"
        );
    }
}

/// Écran et export placent les mêmes boîtes Typst aux mêmes pixels : titre,
/// deux titres d'axe et deux entrées de légende, dont des textes sans `$`.
/// Le témoin est l'export sans Typst, qui n'en place aucune.
#[test]
fn ecran_et_export_placent_les_textes_typst_au_meme_endroit() {
    let (project, spec) = projet_a_legende(["$\"Ru(bpy)\"_3^(2+)$", "Témoin"]);
    let (shapes, _) = ecran(&project, &spec, true);
    let plot = resolve_spec(project.tree(), &spec);
    let measurer = TextMeasurer::default();
    let document = plotyx_render_svg::render_to_string_with_typst(
        &plot,
        (480, 360),
        &measurer,
        &FontChoice::default(),
    )
    .expect("export");
    let ecran = origines_ecran(&shapes);
    assert_eq!(ecran.len(), 5, "titre, deux titres d'axe, deux entrées");
    assert_eq!(ecran, origines_export(&document));

    let sans = plotyx_render_svg::render_to_string(&plot, (480, 360), &measurer).expect("export");
    assert!(
        origines_export(&sans).is_empty(),
        "le témoin a des groupes Typst"
    );
}

/// Une légende mixte — une entrée formule, une sans — a des lignes de même
/// hauteur : les deux textes ont la même boîte (l'étai de
/// `formula::compile`), donc la vignette, centrée sur la ligne, l'est aussi
/// sur chacun des deux.
#[test]
fn une_legende_mixte_a_des_lignes_de_meme_hauteur() {
    let (project, spec) = projet_a_legende(["$\"Ru(bpy)\"_3^(2+)$", "Témoin"]);
    let (shapes, _) = ecran(&project, &spec, true);
    let hauteurs: Vec<f32> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => Some(mesh.calc_bounds()),
            _ => None,
        })
        // Les deux entrées de légende : en haut à droite de la zone de
        // tracé, sous le titre et loin du titre de l'axe du bas.
        .filter(|bounds| bounds.min.x > 240.0 && (40.0..200.0).contains(&bounds.min.y))
        .map(|bounds| bounds.height())
        .collect();
    assert_eq!(hauteurs.len(), 2, "deux entrées attendues : {hauteurs:?}");
    assert!(
        (hauteurs[0] - hauteurs[1]).abs() < 0.01,
        "hauteurs de ligne différentes : {hauteurs:?}"
    );
}

/// Les maillages des entrées de légende (en haut à droite de la zone de
/// tracé), dans l'ordre des lignes.
fn entrees_de_legende(shapes: &[Shape]) -> Vec<egui::Rect> {
    let mut entrees: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) => Some(mesh.calc_bounds()),
            _ => None,
        })
        .filter(|bounds| bounds.min.x > 240.0 && (40.0..200.0).contains(&bounds.min.y))
        .collect();
    entrees.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    entrees
}

/// Issue #33 : un nom de colonne repris en légende est composé par Typst
/// **tel quel**, un nom saisi comme du markup. La même chaîne `$lambda$`
/// donne une formule (une lettre) saisie, et huit caractères reprise d'une
/// colonne. Et le chemin sans Typst affiche le nom d'origine, jamais sa
/// forme échappée `#"…"`.
#[test]
fn un_nom_de_colonne_saffiche_tel_quel_un_nom_saisi_est_compose() {
    let (project, spec) = projet_a_colonnes(["Abs_280 #1", "$lambda$"], ["", "$lambda$"]);
    let (shapes, _) = ecran(&project, &spec, true);
    let [colonne, saisi] = entrees_de_legende(&shapes)[..] else {
        panic!("deux entrées de légende Typst attendues")
    };
    assert!(colonne.width() > 0.0 && saisi.width() > 0.0);

    let (project, repris) = projet_a_colonnes(["Abs_280 #1", "$lambda$"], ["", ""]);
    let (shapes_repris, _) = ecran(&project, &repris, true);
    let [_, litteral] = entrees_de_legende(&shapes_repris)[..] else {
        panic!("deux entrées de légende Typst attendues")
    };
    assert!(
        litteral.width() > 2.0 * saisi.width(),
        "le nom de colonne « $lambda$ » a été lu comme une formule"
    );

    let (sans_typst, _) = ecran(&project, &repris, false);
    let textes: Vec<String> = sans_typst
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        textes.iter().any(|texte| texte == "Abs_280 #1"),
        "{textes:?}"
    );
    assert!(!textes.iter().any(|texte| texte.starts_with("#\"")));
}
