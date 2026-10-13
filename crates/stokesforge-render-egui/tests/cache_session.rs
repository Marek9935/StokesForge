//! Tâche R4.3 vérifiée **de bout en bout** : de la session à la peinture.
//!
//! `src/cache.rs` teste le cache tout seul, sur une clé fabriquée. Ce
//! fichier-ci le teste branché sur ce qui le pilote réellement : un
//! `stokesforge_session::Snapshot`, un `egui::Context`, et une commande du bus
//! qui fait avancer la `revision`. C'est la seule façon de vérifier
//! l'affirmation qui compte — *« une image sans changement ne rejoue pas le
//! pipeline Plotters »* — plutôt qu'une affirmation voisine.
//!
//! Sans serveur graphique : `egui::Context::run_ui` produit des formes sans
//! rien peindre, et c'est tout ce dont on a besoin.

use egui::{Pos2, Rect};
use stokesforge_commands::Command;
use stokesforge_core::column::{Column, ColumnMeta, ColumnType, ValueBlock};
use stokesforge_core::{NodeId, Project};
use stokesforge_plot::{LineSpec, PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_render_egui::{font_definitions, GraphView};
use stokesforge_session::{Session, SessionHandle, Snapshot};

struct Banc {
    session: Session,
    graph: NodeId,
    column: NodeId,
}

impl Banc {
    fn new() -> Self {
        Self::with_title("Cache")
    }

    fn with_title(title: &str) -> Self {
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
            title: title.into(),
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
    // Comme `stokesforge-app` : la source unique de polices est posée
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
        Some(Err(stokesforge_render_egui::RenderError::NotAGraph(_)))
    ));

    let mut inconnu = None;
    consomme(context.run_ui(egui::RawInput::default(), |ui| {
        inconnu = Some(view.paint(ui, rect, &snapshot, NodeId::from_raw(u64::MAX)));
    }));
    assert!(matches!(
        inconnu,
        Some(Err(stokesforge_render_egui::RenderError::UnknownNode(_)))
    ));
}

/// Nombre de maillages texturés parmi les formes en cache — la forme que
/// prend une formule Typst à l'écran (`typst_screen::mesh`), là où un texte
/// ordinaire est un `Shape::Text`.
fn maillages(view: &GraphView) -> usize {
    view.cache()
        .shapes()
        .iter()
        .filter(|shape| matches!(shape, egui::Shape::Mesh(_)))
        .count()
}

/// Les textes restés en `Shape::Text` (mis en page par `epaint`).
fn textes_epaint(view: &GraphView) -> Vec<String> {
    view.cache()
        .shapes()
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

/// Issues #29 et #32 : `GraphView` — donc l'application — passe titres et
/// légende par Typst, **avec ou sans `$`**, et les graduations jamais.
///
/// Les deux moitiés se tiennent l'une l'autre : un maillage venu d'ailleurs
/// (une série, un marqueur) ferait passer « il y a des maillages » sans que
/// Typst ait été consulté, mais pas « le titre n'est plus du texte
/// `epaint` » ; et un backend qui passerait tout par Typst ne laisserait pas
/// les graduations en `Shape::Text`.
#[test]
fn graph_view_passe_titres_et_legende_par_typst() {
    let context = contexte();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));

    for titre in ["Absorbance", "$lambda_\"max\"$ (nm)"] {
        let banc = Banc::with_title(titre);
        let mut vue = GraphView::new();
        image(&context, &mut vue, &banc.snapshot(), banc.graph, rect);
        let textes = textes_epaint(&vue);
        assert!(
            !textes.iter().any(|texte| texte == titre),
            "le titre {titre:?} est resté du texte epaint"
        );
        assert!(maillages(&vue) > 0, "aucun maillage Typst pour {titre:?}");
        assert!(vue.formula_count() > 0);
        assert!(!textes.is_empty(), "plus aucune graduation epaint");
        for texte in &textes {
            assert!(
                texte.parse::<f64>().is_ok(),
                "{texte:?} n'est pas une graduation et n'est pas passé par Typst"
            );
        }
    }
}

/// Issue #29 : une reconstruction de la figure (nouvelle révision) ne
/// recompile pas des textes inchangés, et la mesure d'un texte partage
/// l'entrée de son dessin — une entrée par texte, pas une par appel à
/// `estimate_text_size`.
#[test]
fn reconstruire_la_figure_ne_recompile_pas_un_titre_inchange() {
    let banc = Banc::with_title("$Delta H$");
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));

    image(&context, &mut view, &banc.snapshot(), banc.graph, rect);
    let (formules, maillages_avant) = (view.formula_count(), maillages(&view));
    assert!(formules > 0);

    banc.handle()
        .execute(Command::SetCells {
            column: banc.column,
            start: 3,
            values: ValueBlock::Float(vec![999.0]),
            expected_version: None,
        })
        .expect("commande acceptée");
    assert!(!image(
        &context,
        &mut view,
        &banc.snapshot(),
        banc.graph,
        rect
    ));
    assert_eq!(maillages(&view), maillages_avant);
    assert_eq!(
        view.formula_count(),
        formules,
        "les textes inchangés viennent du cache"
    );
}

/// La taille en pixels de la texture du premier maillage de `shapes`.
fn texture_du_maillage<'a>(
    context: &egui::Context,
    shapes: impl IntoIterator<Item = &'a egui::Shape>,
) -> (egui::TextureId, [usize; 2]) {
    let id = shapes
        .into_iter()
        .find_map(|shape| match shape {
            egui::Shape::Mesh(mesh) => Some(mesh.texture_id),
            _ => None,
        })
        .expect("aucun maillage");
    let size = context
        .tex_manager()
        .read()
        .meta(id)
        .expect("texture inconnue")
        .size;
    (id, size)
}

/// Issue #31, de bout en bout : un graphe affiché deux fois plus grand que
/// sa mise en page logique (`paint_scaled`, comme l'application) peint son
/// titre Typst avec une texture deux fois plus fine que celle du cache,
/// au lieu d'étirer celle-ci. Le témoin est la même vue à sa taille
/// logique : elle peint la texture du cache telle quelle.
#[test]
fn un_graphe_agrandi_peint_ses_formules_a_la_taille_affichee() {
    let banc = Banc::with_title("$Delta H$");
    let context = contexte();
    let mut view = GraphView::new();
    let (largeur, hauteur) = stokesforge_render_egui::DEFAULT_FIGURE_SIZE;
    let logique = egui::vec2(largeur as f32, hauteur as f32);

    let peint = |view: &mut GraphView, taille: egui::Vec2| {
        let mut sortie = context.run_ui(egui::RawInput::default(), |ui| {
            view.paint_scaled(
                ui,
                Rect::from_min_size(Pos2::ZERO, taille),
                &banc.snapshot(),
                banc.graph,
            )
            .expect("peinture");
        });
        let peintes: Vec<egui::Shape> = sortie.shapes.drain(..).map(|c| c.shape).collect();
        sortie.textures_delta.clear();
        texture_du_maillage(&context, &peintes)
    };

    let (temoin, _) = peint(&mut view, logique);
    let formules = view.formula_count();
    let (en_cache, [w, h]) = texture_du_maillage(&context, view.cache().shapes());
    assert_eq!(temoin, en_cache, "à taille logique, la texture du cache");

    let (agrandie, [w2, h2]) = peint(&mut view, logique * 2.0);
    assert_ne!(agrandie, en_cache, "la texture du cache est étirée");
    assert!(
        w2.abs_diff(2 * w) <= 1 && h2.abs_diff(2 * h) <= 1,
        "{:?} n'est pas le double de {:?}",
        [w2, h2],
        [w, h]
    );
    assert_eq!(view.formula_count(), formules, "un texte a été recompilé");
}

// -- Issue #35 : glisser et double-cliquer un texte libre --

/// Un banc dont le panneau porte un texte libre au milieu des données.
fn banc_avec_texte() -> Banc {
    let banc = Banc::with_title("Texte");
    let snapshot = banc.snapshot();
    let mut spec = match snapshot.tree.node(banc.graph).expect("graphe").kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("pas un graphe"),
    };
    if let Some(panel) = spec.panels.get_mut(0) {
        panel
            .annotations
            .push(stokesforge_plot::AnnotationSpec::new("Pic", 16.0, 24.0));
    }
    banc.handle()
        .execute(Command::SetPlotSpec {
            graph: banc.graph,
            spec,
        })
        .expect("spec posé");
    banc
}

/// Une image avec les événements donnés, à l'instant `time` (secondes).
fn image_avec(
    context: &egui::Context,
    view: &mut GraphView,
    banc: &Banc,
    rect: Rect,
    events: Vec<egui::Event>,
    time: f64,
) -> Vec<stokesforge_render_egui::GraphEvent> {
    let input = egui::RawInput {
        events,
        time: Some(time),
        ..Default::default()
    };
    let sortie = context.run_ui(input, |ui| {
        view.paint(ui, rect, &banc.snapshot(), banc.graph)
            .expect("peinture");
    });
    consomme(sortie);
    view.take_events()
}

fn bouton(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Glisser un texte sur plusieurs images : l'aperçu suit le pointeur sans
/// toucher au modèle, et **un seul** signal sort, au relâchement — donc une
/// seule modification du modèle, un seul pas d'historique. Le cadrage de la
/// vue, lui, n'a pas bougé : le glisser n'a pas fait défiler.
#[test]
fn glisser_un_texte_ne_signale_quau_relachement() {
    let banc = banc_avec_texte();
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    let target = stokesforge_plot::AnnotationRef { panel: 0, index: 0 };

    assert!(image_avec(&context, &mut view, &banc, rect, vec![], 0.0).is_empty());
    let (centre, _, _) = view.annotation_screen_box(target).expect("texte tracé");
    let revision = banc.snapshot().revision;
    let generation = view.view().generation();

    let mut signaux = image_avec(
        &context,
        &mut view,
        &banc,
        rect,
        vec![egui::Event::PointerMoved(centre), bouton(centre, true)],
        0.1,
    );
    for pas in 1..=5 {
        let pos = centre + egui::vec2(12.0 * pas as f32, 0.0);
        signaux.extend(image_avec(
            &context,
            &mut view,
            &banc,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            0.1 + 0.05 * pas as f64,
        ));
        assert!(signaux.is_empty(), "un signal avant le relâchement");
    }
    let apercu = view
        .annotation_preview()
        .expect("aperçu pendant le glisser");
    assert!(apercu.1.x > 16.0, "l'aperçu n'a pas suivi le pointeur");
    assert_eq!(
        banc.snapshot().revision,
        revision,
        "le modèle a bougé pendant le glisser"
    );

    let fin = centre + egui::vec2(60.0, 0.0);
    signaux.extend(image_avec(
        &context,
        &mut view,
        &banc,
        rect,
        vec![bouton(fin, false)],
        0.5,
    ));
    let [stokesforge_render_egui::GraphEvent::AnnotationMoved {
        target: deplace,
        annotation,
    }] = &signaux[..]
    else {
        panic!("un seul signal attendu : {signaux:?}")
    };
    assert_eq!(*deplace, target);
    assert!(annotation.x > 16.0 && (annotation.y - 24.0).abs() < 1.0);
    assert_eq!(
        view.view().generation(),
        generation,
        "le glisser a fait défiler"
    );
}

/// Tirer la poignée droite d'un texte sélectionné (#244) fixe sa largeur, en
/// fraction de la figure, symétriquement autour du centre ; un seul signal, au
/// relâchement, et le texte ne bouge pas.
#[test]
fn tirer_la_poignee_fixe_la_largeur() {
    let banc = banc_avec_texte();
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    let target = stokesforge_plot::AnnotationRef { panel: 0, index: 0 };

    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    let (centre, taille, _) = view.annotation_screen_box(target).expect("texte tracé");
    // Un clic sélectionne le texte.
    image_avec(
        &context,
        &mut view,
        &banc,
        rect,
        vec![egui::Event::PointerMoved(centre), bouton(centre, true)],
        0.1,
    );
    image_avec(
        &context,
        &mut view,
        &banc,
        rect,
        vec![bouton(centre, false)],
        0.2,
    );
    assert_eq!(view.selected_annotation(), Some(target));

    let poignee = centre + egui::vec2(taille.x / 2.0 + 3.0, 0.0);
    let cible = centre + egui::vec2(150.0, 0.0);
    let mut signaux = image_avec(
        &context,
        &mut view,
        &banc,
        rect,
        vec![egui::Event::PointerMoved(poignee), bouton(poignee, true)],
        0.3,
    );
    for pas in 1..=5 {
        let pos = poignee + (cible - poignee) * (pas as f32 / 5.0);
        signaux.extend(image_avec(
            &context,
            &mut view,
            &banc,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            0.3 + 0.05 * pas as f64,
        ));
    }
    assert!(signaux.is_empty(), "un signal avant le relâchement");
    signaux.extend(image_avec(
        &context,
        &mut view,
        &banc,
        rect,
        vec![bouton(cible, false)],
        1.0,
    ));
    let [stokesforge_render_egui::GraphEvent::AnnotationMoved { annotation, .. }] = &signaux[..]
    else {
        panic!("un seul signal attendu : {signaux:?}")
    };
    let largeur = annotation.width.expect("largeur fixée");
    assert!((largeur - 0.5).abs() < 0.03, "largeur {largeur}");
    assert_eq!((annotation.x, annotation.y), (16.0, 24.0));
}

/// Un double-clic sur un texte demande à l'éditer et ne réinitialise pas le
/// zoom ; ailleurs, il garde son rôle (réinitialiser).
#[test]
fn double_cliquer_un_texte_demande_a_lediter() {
    let banc = banc_avec_texte();
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    let target = stokesforge_plot::AnnotationRef { panel: 0, index: 0 };
    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    let (centre, _, _) = view.annotation_screen_box(target).expect("texte tracé");
    let mut signaux = Vec::new();
    for (index, (pressed, time)) in [(true, 0.1), (false, 0.12), (true, 0.2), (false, 0.22)]
        .into_iter()
        .enumerate()
    {
        let mut events = vec![bouton(centre, pressed)];
        if index == 0 {
            events.insert(0, egui::Event::PointerMoved(centre));
        }
        signaux.extend(image_avec(&context, &mut view, &banc, rect, events, time));
    }
    assert_eq!(
        signaux,
        vec![stokesforge_render_egui::GraphEvent::EditAnnotation(target)]
    );
    assert_eq!(
        view.selected_annotation(),
        Some(target),
        "le texte est sélectionné"
    );
}

// -- Issue #53 : glisser et double-cliquer un titre, une légende --

/// Glisser le titre du graphe : l'aperçu suit le pointeur sans toucher au
/// modèle, **un seul** signal sort au relâchement, et la vue ne défile pas.
#[test]
fn glisser_un_titre_ne_signale_quau_relachement() {
    use stokesforge_render_egui::{GraphEvent, TextTarget};
    let banc = Banc::with_title("Titre");
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    assert!(image_avec(&context, &mut view, &banc, rect, vec![], 0.0).is_empty());
    let (centre, _, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("titre tracé");
    let revision = banc.snapshot().revision;
    let generation = view.view().generation();

    let mut signaux = image_avec(
        &context,
        &mut view,
        &banc,
        rect,
        vec![egui::Event::PointerMoved(centre), bouton(centre, true)],
        0.1,
    );
    for pas in 1..=5 {
        let pos = centre + egui::vec2(0.0, 20.0 * pas as f32);
        signaux.extend(image_avec(
            &context,
            &mut view,
            &banc,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            0.1 + 0.05 * pas as f64,
        ));
        assert!(signaux.is_empty(), "un signal avant le relâchement");
    }
    let (cible, apercu) = *view.text_preview().expect("aperçu pendant le glisser");
    assert_eq!(cible, TextTarget::Title);
    assert_eq!(banc.snapshot().revision, revision, "le modèle a bougé");

    signaux.extend(image_avec(
        &context,
        &mut view,
        &banc,
        rect,
        vec![bouton(centre + egui::vec2(0.0, 100.0), false)],
        0.5,
    ));
    let [GraphEvent::TextMoved { target, anchor }] = &signaux[..] else {
        panic!("un seul signal attendu : {signaux:?}")
    };
    assert_eq!(*target, TextTarget::Title);
    assert_eq!(*anchor, apercu, "l'ancre validée est celle de l'aperçu");
    assert_eq!(view.view().generation(), generation, "le glisser a défilé");
    // Poser l'ancre dans le modèle : le titre est alors tracé dessus.
    let mut spec = match banc
        .snapshot()
        .tree
        .node(banc.graph)
        .expect("graphe")
        .kind()
    {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("pas un graphe"),
    };
    assert!(TextTarget::Title.set_anchor(&mut spec, Some(*anchor)));
    banc.handle()
        .execute(Command::SetPlotSpec {
            graph: banc.graph,
            spec,
        })
        .expect("spec posé");
    image_avec(&context, &mut view, &banc, rect, vec![], 1.0);
    image_avec(&context, &mut view, &banc, rect, vec![], 1.1);
    let (apres, _, _) = view.text_screen_box(TextTarget::Title).expect("tracé");
    assert!(
        (apres.y - (centre.y + 100.0)).abs() < 3.0,
        "le titre est là où on l'a lâché : {apres:?}"
    );
}

/// Un double-clic sur le titre demande à l'éditer, et ne réinitialise pas le
/// zoom.
#[test]
fn double_cliquer_un_titre_demande_a_lediter() {
    use stokesforge_render_egui::{GraphEvent, TextTarget};
    let banc = Banc::with_title("Titre");
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    let (centre, _, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("titre tracé");
    let mut signaux = Vec::new();
    for (index, (pressed, time)) in [(true, 0.1), (false, 0.12), (true, 0.2), (false, 0.22)]
        .into_iter()
        .enumerate()
    {
        let mut events = vec![bouton(centre, pressed)];
        if index == 0 {
            events.insert(0, egui::Event::PointerMoved(centre));
        }
        signaux.extend(image_avec(&context, &mut view, &banc, rect, events, time));
    }
    assert_eq!(signaux, vec![GraphEvent::EditText(TextTarget::Title)]);
}

/// Issue #77 : un clic sur le titre le sélectionne (et le cadre se peint),
/// un clic dans le vide ne sélectionne plus rien ; la légende se sélectionne
/// par un clic sur l'une de ses entrées, et se déplace par un glisser qui
/// part d'une entrée.
#[test]
fn un_clic_selectionne_un_titre_ou_la_legende() {
    use stokesforge_render_egui::{GraphEvent, TextTarget};
    let banc = Banc::with_title("Titre");
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    assert_eq!(view.selected_text(), None);

    let (titre, _, _) = view
        .text_screen_box(TextTarget::Title)
        .expect("titre tracé");
    let mut temps = 0.1;
    let mut cliquer = |view: &mut GraphView, pos: Pos2| {
        for (pressed, dt) in [(true, 0.0), (false, 0.02)] {
            let mut events = vec![bouton(pos, pressed)];
            if pressed {
                events.insert(0, egui::Event::PointerMoved(pos));
            }
            temps += dt + 0.4;
            image_avec(&context, view, &banc, rect, events, temps);
        }
    };
    cliquer(&mut view, titre);
    assert_eq!(view.selected_text(), Some(TextTarget::Title));

    let legende = TextTarget::Legend { panel: 0 };
    let (centre, taille, _) = view.text_screen_box(legende).expect("légende tracée");
    let entree = centre + egui::vec2(0.0, -taille.y / 2.0 + 14.0);
    cliquer(&mut view, entree);
    assert_eq!(view.selected_text(), Some(legende));

    // Le vide : au centre de la zone de tracé, loin de la légende et du titre.
    cliquer(&mut view, Pos2::new(300.0, 250.0));
    assert_eq!(
        view.selected_text(),
        None,
        "un clic dans le vide désélectionne"
    );

    // Un glisser parti d'une entrée de légende déplace la légende.
    let signaux = glisser(
        &context,
        &mut view,
        &banc,
        rect,
        entree,
        entree + egui::vec2(-60.0, 40.0),
        10.0,
    );
    assert!(
        signaux
            .iter()
            .any(|s| matches!(s, GraphEvent::TextMoved { target, .. } if *target == legende)),
        "{signaux:?}"
    );
    assert_eq!(view.selected_text(), Some(legende));
}

// -- Issue #76 : objets graphiques (rectangle, ellipse, ligne, flèche) --

/// Un banc dont le panneau porte un rectangle **rempli** au milieu des
/// données (le remplissage le rend désignable en son milieu).
fn banc_avec_forme() -> Banc {
    let banc = Banc::with_title("Forme");
    let snapshot = banc.snapshot();
    let mut spec = match snapshot.tree.node(banc.graph).expect("graphe").kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("pas un graphe"),
    };
    if let Some(panel) = spec.panels.get_mut(0) {
        panel.shapes.push(stokesforge_plot::ShapeSpec {
            fill_color: "#3366cc66".to_string(),
            ..stokesforge_plot::ShapeSpec::new(
                stokesforge_plot::ShapeKind::Rectangle,
                10.0,
                15.0,
                20.0,
                35.0,
            )
        });
    }
    banc.handle()
        .execute(Command::SetPlotSpec {
            graph: banc.graph,
            spec,
        })
        .expect("spec posé");
    banc
}

/// Glisse de `from` à `to` en cinq pas ; rend les signaux sortis, dont aucun
/// ne doit précéder le relâchement.
fn glisser(
    context: &egui::Context,
    view: &mut GraphView,
    banc: &Banc,
    rect: Rect,
    from: Pos2,
    to: Pos2,
    mut time: f64,
) -> Vec<stokesforge_render_egui::GraphEvent> {
    let mut signaux = image_avec(
        context,
        view,
        banc,
        rect,
        vec![egui::Event::PointerMoved(from), bouton(from, true)],
        time,
    );
    for pas in 1..=5 {
        time += 0.05;
        let pos = from + (to - from) * (pas as f32 / 5.0);
        signaux.extend(image_avec(
            context,
            view,
            banc,
            rect,
            vec![egui::Event::PointerMoved(pos)],
            time,
        ));
        assert!(signaux.is_empty(), "un signal avant le relâchement");
    }
    signaux.extend(image_avec(
        context,
        view,
        banc,
        rect,
        vec![bouton(to, false)],
        time + 0.05,
    ));
    signaux
}

/// Le centre, à l'écran, de l'objet `index` du premier panneau.
fn forme_a_l_ecran(view: &GraphView, index: usize) -> (Pos2, Pos2) {
    let drawn = view
        .drawn_shape(stokesforge_plot::ShapeRef { panel: 0, index })
        .expect("objet tracé");
    let origin = view.display_rect().expect("rect").min.to_vec2();
    (
        Pos2::new(drawn.p1.0 as f32, drawn.p1.1 as f32) + origin,
        Pos2::new(drawn.p2.0 as f32, drawn.p2.1 as f32) + origin,
    )
}

/// Tracer avec l'outil Rectangle : l'aperçu suit le pointeur sans toucher au
/// modèle, **un seul** signal sort au relâchement (un ajout, au rang qui suit
/// le dernier), et l'outil rend la main au Pointeur.
#[test]
fn tracer_un_rectangle_ne_signale_quau_relachement() {
    let banc = Banc::with_title("Trace");
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    assert!(image_avec(&context, &mut view, &banc, rect, vec![], 0.0).is_empty());
    view.set_tool(stokesforge_render_egui::GraphTool::Rectangle);
    let revision = banc.snapshot().revision;

    let signaux = glisser(
        &context,
        &mut view,
        &banc,
        rect,
        Pos2::new(250.0, 150.0),
        Pos2::new(380.0, 260.0),
        0.1,
    );
    let [stokesforge_render_egui::GraphEvent::ShapeChanged { target, shape }] = &signaux[..] else {
        panic!("un seul signal attendu : {signaux:?}")
    };
    assert_eq!(target.index, 0, "ajout au rang qui suit le dernier");
    assert_eq!(shape.kind, stokesforge_plot::ShapeKind::Rectangle);
    assert!((shape.x2 - shape.x1).abs() > 0.0 && (shape.y2 - shape.y1).abs() > 0.0);
    assert_eq!(banc.snapshot().revision, revision, "le modèle a bougé");
    assert_eq!(view.tool(), stokesforge_render_egui::GraphTool::Pointer);
    assert_eq!(view.selected_shape(), Some(*target));
}

/// Un tremblement de main avec un outil de tracé ne crée rien.
#[test]
fn un_tremblement_avec_un_outil_de_trace_ne_cree_rien() {
    let banc = Banc::with_title("Trace");
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    view.set_tool(stokesforge_render_egui::GraphTool::Arrow);
    let signaux = glisser(
        &context,
        &mut view,
        &banc,
        rect,
        Pos2::new(300.0, 200.0),
        Pos2::new(302.0, 201.0),
        0.1,
    );
    assert!(signaux.is_empty(), "{signaux:?}");
    assert!(view.shape_preview().is_none());
}

/// Glisser un objet rempli le déplace d'un bloc (les deux points, même
/// écart) ; un seul signal, au relâchement ; le cadrage n'a pas bougé.
#[test]
fn glisser_une_forme_la_deplace_en_un_seul_signal() {
    let banc = banc_avec_forme();
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    let (a, b) = forme_a_l_ecran(&view, 0);
    let centre = a.lerp(b, 0.5);
    let generation = view.view().generation();

    let signaux = glisser(
        &context,
        &mut view,
        &banc,
        rect,
        centre,
        centre + egui::vec2(60.0, 0.0),
        0.1,
    );
    let [stokesforge_render_egui::GraphEvent::ShapeChanged { target, shape }] = &signaux[..] else {
        panic!("un seul signal attendu : {signaux:?}")
    };
    assert_eq!(target.index, 0, "remplacement, pas ajout");
    assert!(shape.x1 > 10.0 && shape.x2 > 20.0, "{shape:?}");
    assert!(
        ((shape.x2 - shape.x1) - 10.0).abs() < 1e-6 && ((shape.y2 - shape.y1) - 20.0).abs() < 1e-6,
        "déplacé d'un bloc : la taille ne change pas ({shape:?})"
    );
    assert_eq!(
        view.view().generation(),
        generation,
        "le glisser a fait défiler"
    );
}

/// Une poignée de coin redimensionne : le coin opposé reste en place.
#[test]
fn glisser_une_poignee_redimensionne_la_forme() {
    let banc = banc_avec_forme();
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    let (a, b) = forme_a_l_ecran(&view, 0);
    // Un clic au centre sélectionne l'objet, ce qui fait apparaître ses poignées.
    let centre = a.lerp(b, 0.5);
    let mut time = 0.1;
    for pressed in [true, false] {
        image_avec(
            &context,
            &mut view,
            &banc,
            rect,
            vec![egui::Event::PointerMoved(centre), bouton(centre, pressed)],
            time,
        );
        time += 0.05;
    }
    assert_eq!(view.selected_shape().map(|t| t.index), Some(0));

    let coin_bas_droite = Pos2::new(a.x.max(b.x), a.y.max(b.y));
    let signaux = glisser(
        &context,
        &mut view,
        &banc,
        rect,
        coin_bas_droite,
        coin_bas_droite + egui::vec2(40.0, 30.0),
        time + 0.2,
    );
    let [stokesforge_render_egui::GraphEvent::ShapeChanged { shape, .. }] = &signaux[..] else {
        panic!("un seul signal attendu : {signaux:?}")
    };
    let (gauche, haut) = (shape.x1.min(shape.x2), shape.y1.max(shape.y2));
    assert!(
        (gauche - 10.0).abs() < 0.2,
        "le coin opposé a bougé : {shape:?}"
    );
    assert!(
        (haut - 35.0).abs() < 0.5,
        "le coin opposé a bougé : {shape:?}"
    );
    assert!(
        (shape.x2 - shape.x1).abs() > 10.5,
        "pas agrandi : {shape:?}"
    );
}

// -- Issue #113 : le double-clic ouvre les paramètres de l'objet visé --

/// Un double-clic à `pos` sur quatre images ; rend les signaux émis.
fn double_clic(
    context: &egui::Context,
    view: &mut GraphView,
    banc: &Banc,
    rect: Rect,
    pos: Pos2,
) -> Vec<stokesforge_render_egui::GraphEvent> {
    let mut signaux = Vec::new();
    for (index, (pressed, time)) in [(true, 0.1), (false, 0.12), (true, 0.2), (false, 0.22)]
        .into_iter()
        .enumerate()
    {
        let mut events = vec![bouton(pos, pressed)];
        if index == 0 {
            events.insert(0, egui::Event::PointerMoved(pos));
        }
        signaux.extend(image_avec(context, view, banc, rect, events, time));
    }
    signaux
}

/// Sur un axe : paramètres de l'axe. Sur une courbe : paramètres de la série.
/// Dans le vide de la zone de tracé : le zoom est réinitialisé, comme avant.
#[test]
fn double_cliquer_un_axe_ou_une_courbe_ouvre_ses_parametres() {
    use stokesforge_render::{AxisSide, PlotComponent};
    use stokesforge_render_egui::GraphEvent;
    let banc = Banc::with_title("Titre");
    let context = contexte();
    let mut view = GraphView::new();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    let spec = match banc
        .snapshot()
        .tree
        .node(banc.graph)
        .expect("graphe")
        .kind()
    {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("pas un graphe"),
    };
    let balayage = || {
        (0..600).step_by(2).flat_map(|x| {
            (0..400)
                .step_by(2)
                .map(move |y| Pos2::new(x as f32, y as f32))
        })
    };

    let axe = balayage()
        .find(|pos| {
            matches!(
                view.component_at(*pos),
                Some(PlotComponent::Axis {
                    side: AxisSide::Bottom,
                    ..
                })
            ) && view.series_at(*pos, &spec).is_none()
        })
        .expect("un point de l'axe des abscisses");
    let signaux = double_clic(&context, &mut view, &banc, rect, axe);
    assert!(
        matches!(
            signaux.as_slice(),
            [GraphEvent::OpenAxisProperties {
                side: AxisSide::Bottom,
                ..
            }]
        ),
        "{signaux:?}"
    );

    // Contexte neuf : le temps d'egui ne remonte pas, et le double-clic
    // précédent ne doit pas se mêler au suivant.
    let context = contexte();
    let mut view = GraphView::new();
    image_avec(&context, &mut view, &banc, rect, vec![], 0.0);
    let courbe = balayage()
        .find(|pos| view.series_at(*pos, &spec).is_some())
        .expect("un point de la courbe");
    let attendue = view.series_at(courbe, &spec).expect("série visée");
    let signaux = double_clic(&context, &mut view, &banc, rect, courbe);
    assert_eq!(signaux, vec![GraphEvent::OpenSeriesProperties(attendue)]);
}
