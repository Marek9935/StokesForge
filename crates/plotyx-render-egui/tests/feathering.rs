//! Le levier d'anticrénelage — tâche R5.5 bis, vérifié plutôt qu'affirmé.
//!
//! Deux choses à prouver, et elles ne se prouvent pas au même endroit :
//!
//! 1. **le levier a un effet**, et pas seulement un booléen qui bascule.
//!    C'est la mesure R0 0.3 rejouée en petit : on tessèle la *même* figure
//!    deux fois et on compte les sommets. Si `epaint` changeait un jour de
//!    stratégie de frange, ce test tomberait et la décision du 2026-09-10
//!    serait à réexaminer — ce qui est exactement ce qu'on veut savoir ;
//! 2. **le cache n'en est pas invalidé.** C'est la moitié la plus facile à
//!    perdre : ajouter le régime d'anticrénelage à la clé
//!    `(revision, rect, pixels_per_point)` « par prudence » ferait
//!    reconstruire toute la figure à chaque pression de bouton, donc payer
//!    cher précisément au moment que le levier existe pour alléger
//!    (`architecture-choisie.md` §4.2).

use egui::epaint::Primitive;
use egui::{Pos2, Rect};
use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::{NodeId, Project};
use plotyx_plot::{LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_render_egui::feathering::{self, Feathering};
use plotyx_render_egui::{font_definitions, GraphView};
use plotyx_session::{Session, Snapshot};

const RECT: Rect = Rect {
    min: Pos2::ZERO,
    max: Pos2::new(800.0, 600.0),
};

/// Un projet portant un graphe volontairement chargé en **traits** : c'est
/// la frange des traits que le levier supprime, pas celle du texte.
fn banc() -> (Session, NodeId) {
    let mut project = Project::new("Frange").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");

    let mut panel = PanelSpec::at(0, 0);
    for serie in 0..8 {
        let xs: Vec<f64> = (0..2000).map(|i| i as f64 * 0.01).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| (x + serie as f64).sin() * (1.0 + serie as f64))
            .collect();
        let x = project
            .add_column(sheet, None, Some(&format!("X{serie}")), numeric(&xs))
            .expect("X");
        let y = project
            .add_column(sheet, None, Some(&format!("Y{serie}")), numeric(&ys))
            .expect("Y");
        let mut series = SeriesSpec::new_xy(x, y);
        series.line = LineSpec::from_palette(serie);
        series.show_in_legend = false;
        panel.series.push(series);
    }
    let spec = PlotSpec::empty("Frange").with_panels(panel);
    let graph = project.add_graph(root, None, spec).expect("graphe");
    (Session::spawn(project, None).expect("session"), graph)
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("colonne")
}

fn contexte() -> egui::Context {
    let context = egui::Context::default();
    context.set_fonts(font_definitions());
    context
}

/// Peint une image et rend ses formes, prêtes à tesseler.
fn formes(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
) -> (Vec<egui::epaint::ClippedShape>, bool) {
    let mut depuis_le_cache = false;
    let mut sortie = context.run_ui(egui::RawInput::default(), |ui| {
        depuis_le_cache = view.paint(ui, RECT, snapshot, graph).expect("peinture");
    });
    let shapes = std::mem::take(&mut sortie.shapes);
    // Sans fenêtre, personne ne téléverse l'atlas : `epaint` panique sur un
    // `TexturesDelta` non appliqué (voir `tests/cache_session.rs`).
    sortie.textures_delta.clear();
    (shapes, depuis_le_cache)
}

fn sommets(context: &egui::Context, shapes: Vec<egui::epaint::ClippedShape>) -> usize {
    context
        .tessellate(shapes, 1.0)
        .iter()
        .map(|clipped| match &clipped.primitive {
            Primitive::Mesh(mesh) => mesh.vertices.len(),
            Primitive::Callback(_) => 0,
        })
        .sum()
}

#[test]
fn couper_la_frange_reduit_reellement_la_tessellation() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();
    let (shapes, _) = formes(&context, &mut view, &snapshot, graph);
    assert!(!shapes.is_empty(), "la figure est vide, rien à mesurer");

    feathering::apply(&context, Feathering::Full);
    let avec = sommets(&context, shapes.clone());
    feathering::apply(&context, Feathering::Off);
    let sans = sommets(&context, shapes);

    assert!(
        sans < avec,
        "couper la frange n'a rien changé ({sans} sommets contre {avec}) : \
         le levier de la tâche R5.5 bis n'en est plus un, et la décision du \
         2026-09-10 est à réexaminer"
    );
    // La mesure R0 0.3 donnait 78 % du coût de tessellation ; le rapport de
    // sommets n'est pas le rapport de temps, et la figure d'ici porte aussi
    // du texte, que la frange ne concerne pas. On vérifie donc un ordre de
    // grandeur, pas le chiffre : une frange qui ne coûterait plus que
    // quelques pour cent voudrait dire que le levier a cessé d'être celui
    // qu'on croit.
    let economie = (avec - sans) as f64 / avec as f64;
    assert!(
        economie > 0.25,
        "la frange ne pèse plus que {:.0} % des sommets (avec {avec}, sans {sans})",
        economie * 100.0
    );
}

#[test]
fn le_regime_se_lit_et_se_restaure() {
    let context = contexte();
    let initial = feathering::current(&context);
    assert_eq!(initial, Feathering::Full, "egui part frange active");

    let precedent = feathering::apply(&context, Feathering::Off);
    assert_eq!(precedent, Feathering::Full);
    assert_eq!(feathering::current(&context), Feathering::Off);

    feathering::apply(&context, precedent);
    assert_eq!(feathering::current(&context), Feathering::Full);
}

#[test]
fn basculer_la_frange_ninvalide_pas_le_cache_de_formes() {
    // La propriété que `architecture-choisie.md` §4.2 énonce et que
    // `src/cache.rs` commente : couper la frange change la tessellation, pas
    // les `Shape`. Trois rappels écrits ne valent pas une vérification.
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();

    let (_, premiere) = formes(&context, &mut view, &snapshot, graph);
    assert!(!premiere, "la première image est forcément un calcul");
    let cle = view.cache().key();
    let calculs = view.cache().misses();

    feathering::apply(&context, Feathering::Off);
    let (_, seconde) = formes(&context, &mut view, &snapshot, graph);
    assert!(
        seconde,
        "couper la frange a invalidé le cache : la clé a grossi d'un terme \
         qu'elle ne doit pas porter (architecture-choisie.md §4.2)"
    );

    feathering::apply(&context, Feathering::Full);
    let (_, troisieme) = formes(&context, &mut view, &snapshot, graph);
    assert!(
        troisieme,
        "le rétablissement non plus ne doit rien invalider"
    );

    assert_eq!(view.cache().key(), cle, "la clé de cache a changé");
    assert_eq!(
        view.cache().misses(),
        calculs,
        "le pipeline Plotters a été rejoué alors que seule la frange a bougé"
    );
}
