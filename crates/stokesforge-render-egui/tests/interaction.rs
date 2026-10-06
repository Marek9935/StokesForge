//! La navigation — tâches R7.1 à R7.4, vérifiées par de vrais événements
//! egui rejoués sur plusieurs images plutôt que par un appel direct aux
//! méthodes privées de `GraphView` (elles ne sont justement pas publiques :
//! c'est le câblage entrée egui → `ViewState` qui est à l'épreuve ici, pas
//! la géométrie — déjà couverte, indépendamment d'egui, par
//! `stokesforge-render/src/interact.rs`).
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
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render_egui::{font_definitions, GraphView};
use stokesforge_session::{Session, Snapshot};

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
    // deux bornes doivent baisser (`stokesforge_render::interact`, dérivation
    // détaillée et déjà vérifiée dans `crates/stokesforge-render/src/interact.rs`).
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
    view.set_tool(stokesforge_render_egui::GraphTool::ZoomBox);

    image(&context, &mut view, &snapshot, graph, vec![]);
    // Le cadrage attendu se lit sur la transformation *d'avant* le geste :
    // c'est elle qui traduit les deux coins choisis en données.
    let panel_avant = view
        .last_drawn()
        .expect("une figure a été tracée")
        .panel(0)
        .expect("un panneau")
        .clone();
    let transform_avant = stokesforge_render_egui::PanelTransform::of(&panel_avant)
        .expect("panneau de taille non nulle");

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
        stokesforge_render_egui::PanelTransform::of(panel).expect("panneau de taille non nulle");
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
    view.set_tool(stokesforge_render_egui::GraphTool::Text);
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
            .any(|event| matches!(event, stokesforge_render_egui::GraphEvent::PlaceText { .. })),
        "aucun PlaceText : {events:?}"
    );
    assert_eq!(view.view().generation(), 0);
}

#[test]
fn selection_serie_par_clic_et_arbre_sans_mutation_ni_reconstruction() {
    use stokesforge_plot::SeriesRef;
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graphe")
    };
    let spec = spec.clone();
    let context = contexte();
    let mut view = GraphView::new();
    image(&context, &mut view, &snapshot, graph, vec![]);
    let panel = &view.last_drawn().unwrap().panels[0];
    let point = panel.series_geometry[0].paths[0][50];
    let pos = Pos2::new(point.0 as f32, point.1 as f32);
    assert_eq!(view.series_at(pos, &spec), Some(SeriesRef::new(0, 0)));
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(pos),
            pointer_button(pos, PointerButton::Primary, true),
        ],
    );
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(pos, PointerButton::Primary, false)],
    );
    assert_eq!(view.selected_series(), Some(SeriesRef::new(0, 0)));
    assert!(view.selected_annotation().is_none());
    assert!(view.selected_text().is_none());
    view.select_series(None, &spec);
    assert!(view.selected_series().is_none());
    view.select_series(Some(SeriesRef::new(0, 0)), &spec);
    assert!(
        image(&context, &mut view, &snapshot, graph, vec![]),
        "sélection hors du cache de figure"
    );
    view.select_series(Some(SeriesRef::new(8, 8)), &spec);
    assert!(view.selected_series().is_none());
    assert_eq!(
        session.handle().snapshot().unwrap().revision,
        snapshot.revision
    );
}

#[test]
fn selection_calque_axe_droit_barres_et_rangs_masques() {
    use stokesforge_commands::Command;
    use stokesforge_plot::{PlotType, SeriesAxis, SeriesRef};
    let (session, graph) = banc();
    let handle = session.handle();
    let snapshot = handle.snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graphe")
    };
    let first = spec.panels[0].clone();
    let mut second = first.clone();
    second.row = 1;
    second.series[0].visible = false;
    let mut right = second.series[0].clone();
    right.visible = true;
    right.axis = SeriesAxis::Right;
    second.series.push(right);
    second.right.visible = true;
    second.right.auto_range = false;
    second.right.range_min = Some(0.0);
    second.right.range_max = Some(1000.0);
    let mut spec = spec.clone().with_panels(vec![first, second]);
    handle
        .execute(Command::SetPlotSpec {
            graph,
            spec: spec.clone(),
        })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    let context = contexte();
    let mut view = GraphView::new();
    image(&context, &mut view, &snapshot, graph, vec![]);
    let panel = &view.last_drawn().unwrap().panels[1];
    assert_eq!(panel.series_geometry.len(), 1);
    assert_eq!(panel.series_geometry[0].index, 1);
    let point = panel.series_geometry[0].paths[0][50];
    let pos = Pos2::new(point.0 as f32, point.1 as f32);
    assert_eq!(view.series_at(pos, &spec), Some(SeriesRef::new(1, 1)));
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(pos),
            pointer_button(pos, PointerButton::Primary, true),
        ],
    );
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(pos, PointerButton::Primary, false)],
    );
    assert_eq!(view.selected_series(), Some(SeriesRef::new(1, 1)));
    // Les barres se choisissent aussi à l'intérieur, pas seulement au sommet.
    spec.series_at_mut(SeriesRef::new(1, 1)).unwrap().plot_type = PlotType::Bar;
    handle
        .execute(Command::SetPlotSpec {
            graph,
            spec: spec.clone(),
        })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    image(&context, &mut view, &snapshot, graph, vec![]);
    let panel = &view.last_drawn().unwrap().panels[1];
    let (a, b) = panel.series_geometry[0].bars[50];
    let pos = Pos2::new((a.0 + b.0) as f32 / 2.0, (a.1 + b.1) as f32 / 2.0);
    assert_eq!(view.series_at(pos, &spec), Some(SeriesRef::new(1, 1)));
    // Masquer conserve la sélection de l'arbre et retire toute géométrie.
    spec.series_at_mut(SeriesRef::new(1, 1)).unwrap().visible = false;
    handle
        .execute(Command::SetPlotSpec {
            graph,
            spec: spec.clone(),
        })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    image(&context, &mut view, &snapshot, graph, vec![]);
    assert_eq!(view.selected_series(), Some(SeriesRef::new(1, 1)));
    assert!(view.last_drawn().unwrap().panels[1]
        .series_geometry
        .is_empty());
    assert!(view.series_at(pos, &spec).is_none());
}

#[test]
fn object_manager_selectionne_les_contenus_masques_sans_deux_selections() {
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, SeriesRef, SeriesSpec, ShapeKind, ShapeRef, ShapeSpec,
        TextTarget,
    };
    let mut spec = stokesforge_plot::PlotSpec::empty("Titre");
    let panel = spec.panels.get_mut(0).unwrap();
    panel
        .series
        .push(SeriesSpec::new(stokesforge_core::NodeId::from_raw(900)));
    panel
        .annotations
        .push(AnnotationSpec::new("Texte", 0.0, 0.0));
    panel.annotations[0].visible = false;
    panel
        .shapes
        .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 1.0));
    panel.shapes[0].visible = false;
    spec.title_visible = false;
    let annotation = AnnotationRef {
        row: 0,
        column: 0,
        index: 0,
    };
    let shape = ShapeRef {
        row: 0,
        column: 0,
        index: 0,
    };
    let mut view = stokesforge_render_egui::GraphView::new();
    view.select_series(Some(SeriesRef::new(0, 0)), &spec);
    view.select_annotation(Some(annotation), &spec);
    assert_eq!(view.selected_annotation(), Some(annotation));
    assert_eq!(view.selected_series(), None);
    view.select_shape(Some(shape));
    assert_eq!(view.selected_shape(), Some(shape));
    assert_eq!(view.selected_annotation(), None);
    view.select_text(Some(TextTarget::Title), &spec);
    assert_eq!(view.selected_text(), Some(TextTarget::Title));
    assert_eq!(view.selected_shape(), None);
    view.select_series(Some(SeriesRef::new(0, 0)), &spec);
    assert_eq!(view.selected_text(), None);
    view.select_annotation(
        Some(AnnotationRef {
            index: 99,
            ..annotation
        }),
        &spec,
    );
    assert_eq!(view.selected_annotation(), None);
}

/// Peint une image sans événement et rend les formes produites, aplaties,
/// et si la figure venait du cache.
fn formes(
    context: &egui::Context,
    view: &mut GraphView,
    snapshot: &Snapshot,
    graph: NodeId,
) -> (Vec<egui::Shape>, bool) {
    let mut depuis_le_cache = false;
    let mut sortie = context.run_ui(egui::RawInput::default(), |ui| {
        depuis_le_cache = view.paint(ui, RECT, snapshot, graph).expect("peinture");
    });
    sortie.textures_delta.clear();
    fn aplatir(shape: egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.into_iter().for_each(|s| aplatir(s, out)),
            shape => out.push(shape),
        }
    }
    let mut out = Vec::new();
    for clipped in sortie.shapes {
        aplatir(clipped.shape, &mut out);
    }
    (out, depuis_le_cache)
}

/// Les poignées : carrés pleins de côté `HANDLE_SIDE`, couleur `ink`.
fn poignees(shapes: &[egui::Shape], ink: (u8, u8, u8)) -> Vec<Rect> {
    let side = stokesforge_render::series_selection::HANDLE_SIDE as f32;
    let ink = egui::Color32::from_rgb(ink.0, ink.1, ink.2);
    shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Rect(rect)
                if rect.fill == ink
                    && (rect.rect.width() - side).abs() < 1e-3
                    && (rect.rect.height() - side).abs() < 1e-3 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .collect()
}

/// #98 : une série sélectionnée porte de petits carrés posés le long de son
/// tracé, à l'encre automatique du fond, sans que la figure (le tracé de la
/// courbe compris) change ni soit reconstruite.
#[test]
fn une_serie_selectionnee_porte_des_poignees_sans_changer_son_trace() {
    use stokesforge_commands::Command;
    use stokesforge_plot::SeriesRef;
    use stokesforge_render::color::{DARK_INK, LIGHT_INK};
    use stokesforge_render::series_selection::HANDLE_SPACING;
    let (session, graph) = banc();
    let handle = session.handle();
    let snapshot = handle.snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graphe")
    };
    let mut spec = spec.clone();
    let context = contexte();
    let mut view = GraphView::new();
    let (sans, _) = formes(&context, &mut view, &snapshot, graph);
    assert!(poignees(&sans, DARK_INK).is_empty());

    view.select_series(Some(SeriesRef::new(0, 0)), &spec);
    let (avec, depuis_le_cache) = formes(&context, &mut view, &snapshot, graph);
    assert!(depuis_le_cache, "sélectionner a reconstruit la figure");
    let carres = poignees(&avec, DARK_INK);
    // La diagonale fait plusieurs centaines de pixels : plusieurs poignées,
    // chacune posée sur la courbe tracée.
    assert!(carres.len() >= 5, "{carres:?}");
    let scale = view.display_scale().unwrap();
    let origin = view.display_rect().unwrap().min;
    let path = &view.last_drawn().unwrap().panels[0].series_geometry[0].paths[0];
    let (first, last) = (path[0], path[path.len() - 1]);
    let a = origin + Vec2::new(first.0 as f32, first.1 as f32) * scale;
    let b = origin + Vec2::new(last.0 as f32, last.1 as f32) * scale;
    let direction = (b - a).normalized();
    for carre in &carres {
        let c = carre.center() - a;
        let ecart = (c.x * direction.y - c.y * direction.x).abs();
        assert!(ecart < 2.0, "poignée hors de la courbe : {carre:?}");
    }
    // Espacées de HANDLE_SPACING à l'écran (à l'arrondi au pixel près).
    let pas = carres[0].center().distance(carres[1].center());
    assert!((pas - HANDLE_SPACING as f32).abs() <= 1.5, "pas : {pas}");
    // Le reste de l'image est exactement celui d'avant : la courbe n'est ni
    // repeinte ni surlignée.
    let reste: Vec<_> = avec
        .iter()
        .filter(|shape| !matches!(shape, egui::Shape::Rect(r) if carres.contains(&r.rect)))
        .cloned()
        .collect();
    assert_eq!(reste, sans);

    // Fond sombre : les poignées passent en blanc.
    spec.background = "#101828".to_owned();
    handle
        .execute(Command::SetPlotSpec {
            graph,
            spec: spec.clone(),
        })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    let (sombre, _) = formes(&context, &mut view, &snapshot, graph);
    assert!(poignees(&sombre, DARK_INK).is_empty());
    assert_eq!(poignees(&sombre, LIGHT_INK).len(), carres.len());

    // Désélectionner les retire.
    view.select_series(None, &spec);
    let (apres, _) = formes(&context, &mut view, &snapshot, graph);
    assert!(poignees(&apres, LIGHT_INK).is_empty());
}

/// #98 : les poignées sont un état de la vue, pas du modèle — le spec que
/// l'application exporte (`framed_spec`) est le même, sélection ou non.
#[test]
fn la_selection_n_entre_pas_dans_le_spec_exporte() {
    use stokesforge_plot::SeriesRef;
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graphe")
    };
    let mut view = GraphView::new();
    let avant = view.framed_spec(spec);
    view.select_series(Some(SeriesRef::new(0, 0)), spec);
    assert!(view.selected_series().is_some());
    assert_eq!(view.framed_spec(spec), avant);
}

/// #153 : le clic insère un objet visible, le glisser conserve son tracé
/// libre. Les événements sont publiés une seule fois à la validation.
#[test]
fn les_formes_s_inserent_au_clic_avec_une_taille_visible() {
    use stokesforge_commands::Command;
    use stokesforge_render_egui::{GraphEvent, GraphTool};
    for (tool, width, height) in [
        (GraphTool::Rectangle, 48.0, 48.0),
        (GraphTool::Ellipse, 64.0, 40.0),
        (GraphTool::Line, 80.0, 0.0),
        (GraphTool::Arrow, 80.0, 0.0),
    ] {
        let (session, graph) = banc();
        let handle = session.handle();
        let snapshot = handle.snapshot().unwrap();
        let context = contexte();
        let mut view = GraphView::new();
        image(&context, &mut view, &snapshot, graph, vec![]);
        view.set_tool(tool);
        let at = Pos2::new(350.0, 300.0);
        image(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![
                pointer_moved(at),
                pointer_button(at, PointerButton::Primary, true),
            ],
        );
        assert!(view.take_events().is_empty());
        image(
            &context,
            &mut view,
            &snapshot,
            graph,
            vec![pointer_button(at, PointerButton::Primary, false)],
        );
        let events = view.take_events();
        assert_eq!(events.len(), 1);
        let GraphEvent::ShapeChanged { target, shape } = &events[0] else {
            panic!("insertion")
        };
        assert_eq!(view.tool(), GraphTool::Pointer);
        let stokesforge_core::node::NodeKind::Graph { spec, .. } =
            snapshot.tree.node(graph).unwrap().kind()
        else {
            panic!("graphe")
        };
        let mut updated = spec.clone();
        assert!(target.set(&mut updated, shape.clone()));
        handle
            .execute(Command::SetPlotSpec {
                graph,
                spec: updated,
            })
            .unwrap();
        image(
            &context,
            &mut view,
            &handle.snapshot().unwrap(),
            graph,
            vec![],
        );
        let drawn = view.drawn_shape(*target).unwrap();
        assert!(((drawn.p2.0 - drawn.p1.0).abs() - width).abs() <= 1.0);
        assert!(((drawn.p2.1 - drawn.p1.1).abs() - height).abs() <= 1.0);
        handle.undo().unwrap();
        assert_eq!(
            handle.snapshot().unwrap().tree.node(graph).unwrap().kind(),
            snapshot.tree.node(graph).unwrap().kind()
        );
        handle.redo().unwrap();
        image(
            &context,
            &mut view,
            &handle.snapshot().unwrap(),
            graph,
            vec![],
        );
        assert!(view.drawn_shape(*target).is_some());
    }
}

/// Éprouve le câblage egui de Maj et son changement pendant le geste ;
/// le ratio est celui d'une forme non carrée au départ, jamais celui du défaut.
#[test]
fn maj_contraint_le_redimensionnement_et_se_relache_pendant_le_geste() {
    use stokesforge_commands::Command;
    use stokesforge_plot::{ShapeKind, ShapeRef, ShapeSpec};
    use stokesforge_render_egui::GraphEvent;
    let (session, graph) = banc();
    let handle = session.handle();
    let snapshot = handle.snapshot().unwrap();
    let stokesforge_core::node::NodeKind::Graph { spec, .. } =
        snapshot.tree.node(graph).unwrap().kind()
    else {
        panic!("graphe")
    };
    let mut spec = spec.clone();
    spec.panels.get_mut(0).unwrap().shapes.push(ShapeSpec::new(
        ShapeKind::Rectangle,
        30.0,
        40.0,
        55.0,
        65.0,
    ));
    handle
        .execute(Command::SetPlotSpec { graph, spec })
        .unwrap();
    let snapshot = handle.snapshot().unwrap();
    let context = contexte();
    let mut view = GraphView::new();
    let target = ShapeRef {
        row: 0,
        column: 0,
        index: 0,
    };
    view.select_shape(Some(target));
    image(&context, &mut view, &snapshot, graph, vec![]);
    let drawn = *view.drawn_shape(target).unwrap();
    let ratio = (drawn.p2.0 - drawn.p1.0).abs() / (drawn.p2.1 - drawn.p1.1).abs();
    assert!(
        (ratio - 1.0).abs() > 0.1,
        "forme initiale non carrée à l'écran"
    );
    let scale = view.display_scale().unwrap();
    let origin = view.display_rect().unwrap().min;
    let at = origin
        + Vec2::new(
            drawn.p1.0.max(drawn.p2.0) as f32,
            drawn.p1.1.max(drawn.p2.1) as f32,
        ) * scale;
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![
            pointer_moved(at),
            pointer_button(at, PointerButton::Primary, true),
        ],
    );
    let end = at + Vec2::new(90.0, 15.0);
    let paint = |view: &mut GraphView, shift| {
        let input = egui::RawInput {
            events: vec![
                Event::ModifiersChanged(egui::Modifiers {
                    shift,
                    ..Default::default()
                }),
                pointer_moved(end),
            ],
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            view.paint(ui, RECT, &snapshot, graph).unwrap();
        });
        output.textures_delta.clear();
    };
    paint(&mut view, true);
    assert!(view.take_events().is_empty());
    // L'aperçu est tracé à l'image suivante.
    paint(&mut view, true);
    let resized = view.drawn_shape(target).unwrap();
    let constrained = (resized.p2.0 - resized.p1.0).abs() / (resized.p2.1 - resized.p1.1).abs();
    assert!(
        (ratio - constrained).abs() < 0.03,
        "{ratio} != {constrained}"
    );
    paint(&mut view, false);
    paint(&mut view, false);
    let resized = view.drawn_shape(target).unwrap();
    let free = (resized.p2.0 - resized.p1.0).abs() / (resized.p2.1 - resized.p1.1).abs();
    assert!((ratio - free).abs() > 0.1);
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(end, PointerButton::Primary, false)],
    );
    assert!(
        matches!(view.take_events().as_slice(), [GraphEvent::ShapeChanged { target: changed, .. }] if *changed == target)
    );
}

#[test]
fn glisser_une_forme_garde_le_trace_libre() {
    use stokesforge_render_egui::{GraphEvent, GraphTool};
    let (session, graph) = banc();
    let snapshot = session.handle().snapshot().unwrap();
    let context = contexte();
    let mut view = GraphView::new();
    image(&context, &mut view, &snapshot, graph, vec![]);
    view.set_tool(GraphTool::Rectangle);
    let start = Pos2::new(300.0, 250.0);
    let end = Pos2::new(420.0, 280.0);
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
    assert!(view.take_events().is_empty());
    image(&context, &mut view, &snapshot, graph, vec![]);
    let (target, _) = view.shape_preview().unwrap();
    let drawn = view.drawn_shape(*target).unwrap();
    let scale = view.display_scale().unwrap() as f64;
    assert!(((drawn.p2.0 - drawn.p1.0).abs() * scale - 120.0).abs() < 2.0);
    assert!(((drawn.p2.1 - drawn.p1.1).abs() * scale - 30.0).abs() < 2.0);
    image(
        &context,
        &mut view,
        &snapshot,
        graph,
        vec![pointer_button(end, PointerButton::Primary, false)],
    );
    assert!(matches!(
        view.take_events().as_slice(),
        [GraphEvent::ShapeChanged { .. }]
    ));
}
