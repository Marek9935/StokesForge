//! Chemin écran de Typst — tâche R8.3, et **le test de l'invariant 8.5**.
//!
//! Absent sans la feature `typst` (`architecture-choisie.md` §7) : ce
//! fichier ne compile aucun test dans ce cas, comme les suites `python` du
//! reste du dépôt (`plotyx-python/tests/embedded.rs`, par exemple).

#![cfg(feature = "typst")]

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
    let compiled = formula::compile("$lambda$ (nm)", 14.0, sans, math).unwrap();
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
