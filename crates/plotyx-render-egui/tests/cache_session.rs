//! Tâche R4.3 vérifiée **de bout en bout** : de la session à la peinture.
//!
//! `src/cache.rs` teste le cache tout seul, sur une clé fabriquée. Ce
//! fichier-ci le teste branché sur ce qui le pilote réellement : un
//! `plotyx_session::Snapshot`, un `egui::Context`, et une commande du bus
//! qui fait avancer la `revision`. C'est la seule façon de vérifier
//! l'affirmation qui compte — *« une image sans changement ne rejoue pas le
//! pipeline Plotters »* — plutôt qu'une affirmation voisine.
//!
//! Sans serveur graphique : `egui::Context::run_ui` produit des formes sans
//! rien peindre, et c'est tout ce dont on a besoin.

use egui::{Pos2, Rect};
use plotyx_commands::Command;
use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::{NodeId, Project};
use plotyx_plot::{LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use plotyx_render_egui::{font_definitions, GraphView};
use plotyx_session::{Session, SessionHandle, Snapshot};

struct Banc {
    session: Session,
    graph: NodeId,
    column: NodeId,
}

impl Banc {
    fn new() -> Self {
        let mut project = Project::new("Cache").expect("projet");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("classeur");
        let sheet = project.add_worksheet(book, None).expect("feuille");
        let xs: Vec<f64> = (0..32).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| x * 1.5).collect();
        let x = project
            .add_column(sheet, None, Some("X"), numeric(&xs))
            .expect("X");
        let column = project
            .add_column(sheet, None, Some("Y"), numeric(&ys))
            .expect("Y");

        let mut panel = PanelSpec::at(0, 0);
        let mut series = SeriesSpec::new_xy(x, column);
        series.line = LineSpec::from_palette(0);
        panel.series.push(series);
        let spec = PlotSpec {
            title: "Cache".into(),
            ..PlotSpec::empty("")
        }
        .with_panels(panel);
        let graph = project.add_graph(root, None, spec).expect("graphe");

        let session = Session::spawn(project, None).expect("session");
        Self {
            session,
            graph,
            column,
        }
    }

    fn handle(&self) -> SessionHandle {
        self.session.handle()
    }

    fn snapshot(&self) -> Snapshot {
        self.handle().snapshot().expect("instantané")
    }
}

fn numeric(values: &[f64]) -> Column {
    Column::from_block(
        ColumnType::Numeric,
        ValueBlock::Float(values.to_vec()),
        ColumnMeta::default(),
    )
    .expect("colonne")
}

/// Peint une image et rend `true` si elle est venue du cache.
///
/// Le `Context` est celui qu'on réutilise d'une image à l'autre : c'est ce
/// qui rend l'expérience honnête, un `Context` neuf à chaque appel n'ayant
/// ni le même atlas ni forcément le même `pixels_per_point`.
fn image(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
    rect: Rect,
) -> bool {
    let mut depuis_le_cache = false;
    let sortie = context.run_ui(egui::RawInput::default(), |ui| {
        depuis_le_cache = view.paint(ui, rect, snapshot, graph).expect("peinture");
    });
    consomme(sortie);
    depuis_le_cache
}

/// Sans fenêtre, personne ne téléverse l'atlas de glyphes — et `epaint`
/// **panique** si on laisse tomber un `TexturesDelta` non appliqué
/// (`textures.rs` : *« Deltas need to be handled »*). C'est une précaution
/// utile en vrai, et un piège à connaître pour tester le rendu sans serveur
/// graphique : il faut déclarer la texture consommée.
fn consomme(mut sortie: egui::FullOutput) {
    sortie.textures_delta.clear();
}

fn contexte() -> egui::Context {
    let context = egui::Context::default();
    // Comme `plotyx-app` : la source unique de polices est posée
    // explicitement, pas héritée du défaut d'egui.
    context.set_fonts(font_definitions());
    context
}

#[test]
fn une_image_sans_changement_ne_rejoue_pas_le_pipeline() {
    let banc = Banc::new();
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(600.0, 400.0));
    let snapshot = banc.snapshot();

    assert!(
        !image(&context, &mut view, &snapshot, banc.graph, rect),
        "la première image doit construire"
    );
    for _ in 0..5 {
        assert!(
            image(&context, &mut view, &snapshot, banc.graph, rect),
            "une image suivante, à instantané et rectangle identiques, doit \
             venir du cache"
        );
    }
    assert_eq!(view.cache().misses(), 1, "un seul calcul pour six images");
    assert_eq!(view.cache().hits(), 5);
    assert!(!view.cache().shapes().is_empty());
    assert_eq!(
        view.last_drawn().map(|drawn| drawn.drawn_series),
        Some(1),
        "la série devrait avoir été tracée"
    );
}

#[test]
fn une_commande_sur_une_colonne_referencee_invalide_le_cache() {
    // C'est le cœur du protocole : le renderer est un client de plus de la
    // `revision` de la session, au même titre que l'UI et le fil de script
    // (`architecture-choisie.md` §4.2).
    let banc = Banc::new();
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));

    let avant = banc.snapshot();
    image(&context, &mut view, &avant, banc.graph, rect);
    assert!(image(&context, &mut view, &avant, banc.graph, rect));

    banc.handle()
        .execute(Command::SetCells {
            column: banc.column,
            start: 3,
            values: ValueBlock::Float(vec![999.0]),
            expected_version: None,
        })
        .expect("commande acceptée");

    let apres = banc.snapshot();
    assert_ne!(
        apres.revision, avant.revision,
        "la révision doit avoir avancé"
    );
    assert!(
        !image(&context, &mut view, &apres, banc.graph, rect),
        "un nouvel instantané doit faire rejouer le pipeline"
    );
    assert_eq!(view.cache().misses(), 2);
}

#[test]
fn redimensionner_invalide_le_cache() {
    let banc = Banc::new();
    let context = contexte();
    let mut view = GraphView::new();
    let snapshot = banc.snapshot();

    let petit = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
    let grand = Rect::from_min_size(Pos2::ZERO, egui::vec2(401.0, 300.0));
    image(&context, &mut view, &snapshot, banc.graph, petit);
    assert!(image(&context, &mut view, &snapshot, banc.graph, petit));
    assert!(
        !image(&context, &mut view, &snapshot, banc.graph, grand),
        "un pixel de large en plus change la géométrie de la figure"
    );
}

#[test]
fn un_noeud_qui_nest_pas_un_graphe_est_une_erreur_pas_un_plantage() {
    let banc = Banc::new();
    let context = contexte();
    let mut view = GraphView::new();
    let snapshot = banc.snapshot();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));

    let mut resultat = None;
    consomme(context.run_ui(egui::RawInput::default(), |ui| {
        resultat = Some(view.paint(ui, rect, &snapshot, banc.column));
    }));
    assert!(matches!(
        resultat,
        Some(Err(plotyx_render_egui::RenderError::NotAGraph(_)))
    ));

    let mut inconnu = None;
    consomme(context.run_ui(egui::RawInput::default(), |ui| {
        inconnu = Some(view.paint(ui, rect, &snapshot, NodeId::from_raw(u64::MAX)));
    }));
    assert!(matches!(
        inconnu,
        Some(Err(plotyx_render_egui::RenderError::UnknownNode(_)))
    ));
}
