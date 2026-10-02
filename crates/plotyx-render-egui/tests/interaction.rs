//! La navigation — tâches R7.1 à R7.4, vérifiées par de vrais événements
//! egui rejoués sur plusieurs images plutôt que par un appel direct aux
//! méthodes privées de `GraphView` (elles ne sont justement pas publiques :
//! c'est le câblage entrée egui → `ViewState` qui est à l'épreuve ici, pas
//! la géométrie — déjà couverte, indépendamment d'egui, par
//! `plotyx-render/src/interact.rs`).
//!
//! # Pourquoi deux images pour un geste
//!
//! `egui` calcule les interactions d'une image à partir des rectangles de
//! widgets **de l'image précédente** (`egui::interaction` : *"Calculated at
//! the start of each frame based on: Widget rects from previous frame"*).
//! Une première image « à blanc » (aucun événement) sert donc uniquement à
//! faire exister le rectangle interactif de la vue ; les événements de
//! pointeur eux-mêmes sont envoyés à partir de la deuxième.

use egui::{Event, MouseWheelUnit, PointerButton, Pos2, Rect, Vec2};
use plotyx_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use plotyx_core::{NodeId, Project};
use plotyx_plot::{PanelSpec, PlotSpec, SeriesSpec};
use plotyx_render_egui::{font_definitions, GraphView};
use plotyx_session::{Session, Snapshot};

const RECT: Rect = Rect {
    min: Pos2::ZERO,
    max: Pos2::new(800.0, 600.0),
};

/// Un graphe à une série, cadrée sur `0..100` des deux côtés une fois la
/// marge de 5 % de `pipeline::padded` retirée mentalement — les tests ne
/// dépendent que du signe des mouvements, jamais de la marge exacte.
fn banc() -> (Session, NodeId) {
    let mut project = Project::new("Navigation").expect("projet");
    let root = project.root();
    let book = project.add_empty_workbook(root, None).expect("classeur");
    let sheet = project.add_worksheet(book, None).expect("feuille");
    let xs: Vec<f64> = (0..=100).map(|i| i as f64).collect();
    let ys = xs.clone();
    let x = project
        .add_column(sheet, None, Some("X"), numeric(&xs))
        .expect("X");
    let y = project
        .add_column(sheet, None, Some("Y"), numeric(&ys))
        .expect("Y");
    let panel = {
        let mut panel = PanelSpec::at(0, 0);
        panel.series.push(SeriesSpec::new_xy(x, y));
        panel
    };
    let spec = PlotSpec::empty("Navigation").with_panels(panel);
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

/// Peint une image avec les événements donnés, et rend si elle vient du
/// cache. Consomme systématiquement le `TexturesDelta` : sans fenêtre,
/// personne d'autre ne le fait, et `epaint` panique sur un delta laissé de
/// côté (voir `tests/cache_session.rs`).
fn image(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
    events: Vec<Event>,
) -> bool {
    let mut depuis_le_cache = false;
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    let mut sortie = context.run_ui(input, |ui| {
        depuis_le_cache = view.paint(ui, RECT, snapshot, graph).expect("peinture");
    });
    sortie.textures_delta.clear();
    depuis_le_cache
}

fn pointer_moved(pos: Pos2) -> Event {
    Event::PointerMoved(pos)
}

fn pointer_button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

/// Un défilement dont l'unité est `Point` et l'amplitude **sous 8,0** :
/// c'est la condition exacte (`egui::input_state::wheel_state`,
/// `is_smooth`) sous laquelle `smooth_scroll_delta` reçoit le delta entier
/// dès l'image où l'événement arrive, sans lissage sur plusieurs images — ce
/// qui rend le test déterministe en une seule image. Au-delà de 8,0, `egui`
/// étale le delta sur plusieurs images (pensé pour une molette physique, qui
/// avance par grands crans), et le test devrait attendre plusieurs images
/// pour voir tout l'effet.
fn wheel(delta_y: f32) -> Event {
    wheel_with(delta_y, egui::Modifiers::CTRL)
}

fn wheel_with(delta_y: f32, modifiers: egui::Modifiers) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: Vec2::new(0.0, delta_y),
        modifiers,
        phase: egui::TouchPhase::Move,
    }
}

/// Un point du panneau, en pixel — le centre du rectangle du test,
/// arbitraire mais fixe.
const SOUS_CURSEUR: Pos2 = Pos2::new(400.0, 300.0);

#[test]
fn zoomer_a_la_molette_retrecit_le_cadrage_et_fait_avancer_la_generation() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();

    // Image 0 : à blanc, pour que le rectangle interactif existe.
    image(&context, &mut view, &snapshot, graph, vec![]);
    let avant = view
        .last_drawn()
        .expect("une figure a été tracée")
        .x_range();
    assert_eq!(view.view().generation(), 0);

    // Image 1 : la molette, survolée au-dessus de la figure. Le signe est
    // celui qui, avec `handle_navigation` (`facteur = exp(-défilement × k)`),
    // donne un facteur `< 1` — donc un rétrécissement ; l'inverse
    // agrandirait, ce que `wheel_state.rs` ne distingue pas physiquement
    // d'un sens ou de l'autre (aucune convention n'est imposée par R7).
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(SOUS_CURSEUR), wheel(7.9)],
    );
    // Il faut une deuxième image pour que le pipeline rejoue avec le nouveau
    // cadrage (la génération a changé la clé de cache **pendant** l'image où
    // le geste a eu lieu, donc `outcome.cached` de cette image-là peut
    // encore porter l'ancienne figure selon l'ordre exact des étapes ; ce
    // test ne fixe que le résultat stable, une image plus tard).
    image(&context, &mut view, &snapshot, graph, vec![]);

    assert!(
        view.view().generation() > 0,
        "la molette n'a pas fait avancer la génération de vue"
    );
    let apres = view
        .last_drawn()
        .expect("une figure a été tracée")
        .x_range();
    assert!(
        (apres.1 - apres.0) < (avant.1 - avant.0),
        "le cadrage n'a pas rétréci : avant {avant:?}, après {apres:?}"
    );
}

#[test]
fn glisser_le_bouton_principal_deplace_le_cadrage() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();

    image(&context, &mut view, &snapshot, graph, vec![]);
    let avant = view
        .last_drawn()
        .expect("une figure a été tracée")
        .x_range();

    let start = Pos2::new(300.0, 300.0);
    let end = Pos2::new(500.0, 300.0);

    // Presser sur le rectangle interactif de l'image précédente.
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(start),
            pointer_button(start, PointerButton::Primary, true),
        ],
    );
    // Bouger, bouton toujours enfoncé (l'état « pressé » persiste tant
    // qu'aucun événement de relâchement n'arrive).
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(end)],
    );
    // Relâcher, et laisser une image de plus stabiliser la figure.
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(end, PointerButton::Primary, false)],
    );
    image(&context, &mut view, &snapshot, graph, vec![]);

    assert!(
        view.view().generation() > 0,
        "le glissé n'a fait avancer aucune génération de vue"
    );
    let apres = view
        .last_drawn()
        .expect("une figure a été tracée")
        .x_range();
    // Glisser vers la droite fait apparaître ce qui était à gauche : les
    // deux bornes doivent baisser (`plotyx_render::interact`, dérivation
    // détaillée et déjà vérifiée dans `crates/plotyx-render/src/interact.rs`).
    assert!(
        apres.0 < avant.0 && apres.1 < avant.1,
        "le glissé n'a pas déplacé le cadrage dans le sens attendu : \
         avant {avant:?}, après {apres:?}"
    );
    // La largeur du cadrage, elle, ne doit pas avoir bougé : un pan n'est
    // pas un zoom.
    assert!(
        ((apres.1 - apres.0) - (avant.1 - avant.0)).abs() < 1e-6,
        "un pan a changé la largeur du cadrage : avant {avant:?}, après {apres:?}"
    );
}

#[test]
fn le_rectangle_elastique_zoome_sur_la_zone_choisie() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();
    view.set_tool(plotyx_render_egui::GraphTool::ZoomBox);

    image(&context, &mut view, &snapshot, graph, vec![]);
    // Le cadrage attendu se lit sur la transformation *d'avant* le geste :
    // c'est elle qui traduit les deux coins choisis en données.
    let panel_avant = view
        .last_drawn()
        .expect("une figure a été tracée")
        .panel(0)
        .expect("un panneau")
        .clone();
    let transform_avant =
        plotyx_render_egui::PanelTransform::of(&panel_avant).expect("panneau de taille non nulle");

    let start = Pos2::new(300.0, 200.0);
    let end = Pos2::new(500.0, 400.0);
    let attendu = transform_avant
        .range_between(
            (start.x as f64, start.y as f64),
            (end.x as f64, end.y as f64),
        )
        .expect("les deux coins sont dans le cadrage");

    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(start),
            pointer_button(start, PointerButton::Primary, true),
        ],
    );
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(end)],
    );
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(end, PointerButton::Primary, false)],
    );
    image(&context, &mut view, &snapshot, graph, vec![]);

    assert!(
        view.view().generation() > 0,
        "le rectangle élastique n'a fait avancer aucune génération de vue"
    );
    let framing = view
        .view()
        .framing(panel_avant.row, panel_avant.column)
        .expect("le panneau devrait porter un cadrage explicite");
    assert!(
        (framing.x.0 - attendu.0 .0).abs() < 1e-6 && (framing.x.1 - attendu.0 .1).abs() < 1e-6,
        "x_range = {:?}, attendu {:?}",
        framing.x,
        attendu.0
    );
    assert!(
        (framing.y.0 - attendu.1 .0).abs() < 1e-6 && (framing.y.1 - attendu.1 .1).abs() < 1e-6,
        "y_range = {:?}, attendu {:?}",
        framing.y,
        attendu.1
    );
}

#[test]
fn survoler_ne_grossit_pas_le_compteur_de_calculs() {
    // La propriété centrale de la tâche 7.3 : le réticule se peint
    // par-dessus le cache, sans l'invalider (`crate::cache`, tête de
    // module). Bouger uniquement le curseur ne doit provoquer aucun
    // nouveau calcul du pipeline Plotters.
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();

    let premiere = image(&context, &mut view, &snapshot, graph, vec![]);
    assert!(!premiere, "la première image est forcément un calcul");
    let calculs = view.cache().misses();

    for pos in [
        Pos2::new(200.0, 200.0),
        Pos2::new(210.0, 205.0),
        Pos2::new(600.0, 450.0),
        Pos2::new(400.0, 300.0),
    ] {
        let depuis_le_cache = image(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![pointer_moved(pos)],
        );
        assert!(depuis_le_cache, "un survol a déclenché un calcul à {pos:?}");
    }

    assert_eq!(
        view.cache().misses(),
        calculs,
        "bouger le réticule a fait grimper le nombre de calculs du pipeline"
    );
}

#[test]
fn survoler_un_point_connu_le_designe() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();

    image(&context, &mut view, &snapshot, graph, vec![]);
    let panel = view
        .last_drawn()
        .expect("une figure a été tracée")
        .panel(0)
        .expect("un panneau");
    let transform =
        plotyx_render_egui::PanelTransform::of(panel).expect("panneau de taille non nulle");
    // La donnée (50, 50) existe dans la série (`banc`, X = Y = 0..=100) :
    // son pixel est un point de désignation garanti, contrairement à un
    // pixel choisi au hasard qui pourrait tomber entre deux points sur une
    // figure décimée.
    let (px, py) = transform
        .data_to_pixel(50.0, 50.0)
        .expect("(50, 50) est dans le cadrage");
    let pixel = Pos2::new(px as f32, py as f32);

    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(pixel)],
    );

    let hit = view
        .hovered_point()
        .expect("un point aurait dû être désigné");
    assert_eq!(hit.data, (50.0, 50.0));
    assert!(hit.distance < 1.0, "distance = {}", hit.distance);
}

#[test]
fn double_cliquer_reinitialise_le_zoom() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();

    image(&context, &mut view, &snapshot, graph, vec![]);
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_moved(SOUS_CURSEUR), wheel(7.9)],
    );
    image(&context, &mut view, &snapshot, graph, vec![]);
    assert!(!view.view().is_empty(), "le zoom n'a pas posé de cadrage");

    let double_clic = vec![
        pointer_moved(SOUS_CURSEUR),
        pointer_button(SOUS_CURSEUR, PointerButton::Primary, true),
        pointer_button(SOUS_CURSEUR, PointerButton::Primary, false),
        pointer_button(SOUS_CURSEUR, PointerButton::Primary, true),
        pointer_button(SOUS_CURSEUR, PointerButton::Primary, false),
    ];
    image(&context, &mut view, &snapshot, graph, double_clic);
    image(&context, &mut view, &snapshot, graph, vec![]);

    assert!(
        view.view().is_empty(),
        "le double-clic n'a pas réinitialisé le cadrage"
    );
}

#[test]
fn la_molette_seule_ne_zoome_plus() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();
    image(&context, &mut view, &snapshot, graph, vec![]);
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(SOUS_CURSEUR),
            wheel_with(7.9, egui::Modifiers::default()),
        ],
    );
    image(&context, &mut view, &snapshot, graph, vec![]);
    assert_eq!(view.view().generation(), 0, "la molette seule a zoomé");
}

#[test]
fn le_glisser_secondaire_ne_trace_plus_de_rectangle() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();
    image(&context, &mut view, &snapshot, graph, vec![]);
    let (start, end) = (Pos2::new(300.0, 200.0), Pos2::new(500.0, 400.0));
    for events in [
        vec![
            pointer_moved(start),
            pointer_button(start, PointerButton::Secondary, true),
        ],
        vec![pointer_moved(end)],
        vec![pointer_button(end, PointerButton::Secondary, false)],
        vec![],
    ] {
        image(&context, &mut view, &snapshot, graph, events);
    }
    assert_eq!(view.view().generation(), 0, "le clic droit a zoomé");
}

#[test]
fn l_outil_texte_demande_un_texte_au_clic_et_ne_fait_pas_defiler() {
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().expect("instantané");
    let context = contexte();
    let mut view = GraphView::new();
    view.set_tool(plotyx_render_egui::GraphTool::Text);
    image(&context, &mut view, &snapshot, graph, vec![]);
    for events in [
        vec![
            pointer_moved(SOUS_CURSEUR),
            pointer_button(SOUS_CURSEUR, PointerButton::Primary, true),
        ],
        vec![pointer_button(SOUS_CURSEUR, PointerButton::Primary, false)],
        vec![],
    ] {
        image(&context, &mut view, &snapshot, graph, events);
    }
    let events = view.take_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, plotyx_render_egui::GraphEvent::PlaceText { .. })),
        "aucun PlaceText : {events:?}"
    );
    assert_eq!(view.view().generation(), 0);
}
