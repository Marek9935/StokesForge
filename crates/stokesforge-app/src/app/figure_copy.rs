//! Copier la figure du graphique actif vers une application externe (Ctrl+C,
//! issue #149).
//!
//! # Le geste
//!
//! Ctrl+C n'arrive jamais comme un `Event::Key` : `egui-winit` le traduit en
//! `Event::Copy` (voir `grid.rs`, `grid_clipboard_shortcuts`). La figure
//! entière est copiée quand, **ensemble** :
//!
//! - une fenêtre de graphique est active, au premier plan — flottante (celle
//!   que désigne `active_content_tab`) ou détachée (son viewport reçoit la
//!   touche, donc il est le focalisé) ;
//! - aucun objet du graphique n'est sélectionné (`selected_manager_target` :
//!   série, texte libre, forme, titre ou légende ; la sélection de l'Object
//!   Manager est celle de la vue) : le copier-coller d'un objet n'est pas
//!   défini par #149 ;
//! - aucun champ de saisie n'a le focus (`keyboard_focus_outside_grid`) : un
//!   Ctrl+C dans un champ copie son texte.
//!
//! Le Project Manager retire lui-même son `Event::Copy` quand il a le focus, et
//! ce module passe **avant** les panneaux et les contenus (le raccourci est
//! consommé au niveau du contexte, comme `collect_shortcuts`) : une grille
//! visible derrière la fenêtre de graphique ne reçoit donc pas le même
//! `Event::Copy` et n'écrase pas la figure par du texte de cellules.
//!
//! # Ce que ce module ne fait pas
//!
//! Il ne rend ni n'offre rien : il pousse une [`Action::CopyGraphFigure`] dont
//! le spec est déjà cadré par la vue (le zoom affiché, comme l'export vers un
//! fichier). `AppState` prépare les octets, et [`StokesForgeApp::offer_figure`]
//! les offre au système depuis la fenêtre racine, la seule à connaître son
//! affichage (`eframe::Frame`). Aucune commande, aucune entrée d'historique.

use eframe::egui;
use stokesforge_clipboard::{mime, ClipboardError, Representation};
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::GraphView;
use stokesforge_render_svg::ExportFormat;

use crate::state::Action;
use crate::tr;

use super::grid::keyboard_focus_outside_grid;
use super::object_manager_ui::selected_manager_target;
use super::tab::Tab;
use super::StokesForgeApp;

/// Vrai si un `Event::Copy` attend dans l'entrée du contexte — la lecture bon
/// marché qui évite tout travail les images sans Ctrl+C.
pub(super) fn copy_event_pending(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Copy))
    })
}

/// Le geste « copier la figure » est-il celui-ci ? Si oui, il **retire** l'
/// `Event::Copy` pour que personne d'autre ne le traite.
///
/// `view` est la vue du graphique que le geste viserait : sa sélection décide.
pub(super) fn take_figure_copy_gesture(ctx: &egui::Context, view: &GraphView) -> bool {
    if keyboard_focus_outside_grid(ctx) || selected_manager_target(view).is_some() {
        return false;
    }
    ctx.input_mut(|input| {
        let before = input.events.len();
        input
            .events
            .retain(|event| !matches!(event, egui::Event::Copy));
        input.events.len() != before
    })
}

/// Pousse [`Action::CopyGraphFigure`] si le geste est celui de la figure de
/// `graph`, cadrée par `view`.
pub(super) fn request_figure_copy(
    ctx: &egui::Context,
    view: &GraphView,
    tree: &Tree,
    graph: NodeId,
    actions: &mut Vec<Action>,
) {
    if !copy_event_pending(ctx) || !take_figure_copy_gesture(ctx, view) {
        return;
    }
    if let Some(spec) = stokesforge_core::compose::current_plot_spec(tree, graph) {
        actions.push(Action::CopyGraphFigure {
            spec: Box::new(view.framed_spec(&spec)),
        });
    }
}

impl StokesForgeApp {
    /// Ctrl+C sur le graphique flottant actif (la racine ne reçoit la touche
    /// que si elle a le focus ; un graphique détaché lit la sienne dans son
    /// viewport, voir `window_detach.rs`).
    pub(super) fn request_active_figure_copy(
        &mut self,
        ctx: &egui::Context,
        active_tab: Option<Tab>,
        actions: &mut Vec<Action>,
    ) {
        let Some(tab @ Tab::Graph(graph)) = active_tab else {
            return;
        };
        if self.detached_tabs.contains_key(&tab) || !copy_event_pending(ctx) {
            return;
        }
        let tree = self.state.snapshot().tree.clone();
        self.with_graph_view(graph, |view| {
            request_figure_copy(ctx, view, &tree, graph, actions)
        });
    }

    /// L'entrée « Copier le graphique » des menus : la même action que Ctrl+C,
    /// sans condition de sélection (le geste est explicite), avec le cadrage de
    /// la vue du graphique `graph`, flottante ou détachée.
    pub(super) fn request_graph_figure_copy(&mut self, graph: NodeId, actions: &mut Vec<Action>) {
        let tree = self.state.snapshot().tree.clone();
        self.with_graph_view(graph, |view| {
            if let Some(spec) = stokesforge_core::compose::current_plot_spec(&tree, graph) {
                actions.push(Action::CopyGraphFigure {
                    spec: Box::new(view.framed_spec(&spec)),
                });
            }
        });
    }

    /// Offre au système les représentations que [`Action::CopyGraphFigure`]
    /// vient de préparer. Un refus remplace le message « copié » par sa cause.
    pub(super) fn offer_figure(&mut self, frame: &eframe::Frame) {
        let Some(figures) = self.state.take_figure_clipboard() else {
            return;
        };
        let representations: Vec<Representation> = figures
            .into_iter()
            .map(|figure| Representation::new(mime_of(figure.format), figure.bytes))
            .collect();
        if let Err(error) = stokesforge_clipboard::offer(frame, &representations) {
            self.state
                .figure_clipboard_failed(clipboard_error_message(&error));
        }
    }
}

/// Le type MIME sous lequel la figure d'un format d'export est offerte. Un
/// `match` exhaustif, sans `_` : un format nouveau ne compile pas tant qu'il
/// n'a pas son type.
fn mime_of(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Svg { .. } => mime::SVG,
        ExportFormat::Png { .. } => mime::PNG,
        ExportFormat::Pdf => mime::PDF,
    }
}

/// La phrase d'une [`ClipboardError`] : les erreurs de l'offre multi-format au
/// presse-papiers système (#149), mises en phrase ici parce que
/// `stokesforge-clipboard` (et son winit) reste dans l'application.
pub(super) fn clipboard_error_message(error: &ClipboardError) -> String {
    match error {
        ClipboardError::Empty => tr!("error-clipboard-empty"),
        ClipboardError::Unsupported(display) => {
            tr!("error-clipboard-unsupported", display = display.to_string())
        }
        ClipboardError::Backend(detail) => {
            tr!("error-clipboard-backend", detail = detail.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::graph_window::{graph_view_contents, tests::graphe};
    use super::super::test_support::{cliquer, contexte, image, image_de};
    use super::*;
    use crate::errors::tests_support::check;
    use crate::state::AppState;
    use stokesforge_session::Session;

    /// Chaque format d'export offert a son type MIME, distinct des autres
    /// (témoin : deux formats qui partageraient un type seraient indiscernables
    /// pour l'application destinataire).
    #[test]
    fn chaque_format_de_la_figure_a_son_type_mime() {
        use stokesforge_render_svg::SvgTextMode;
        let svg = ExportFormat::Svg {
            text: SvgTextMode::Text,
        };
        let png = ExportFormat::Png { scale: 2.0 };
        assert_eq!(mime_of(svg), "image/svg+xml");
        assert_eq!(mime_of(png), "image/png");
        assert_eq!(mime_of(ExportFormat::Pdf), "application/pdf");
        // L'échelle du PNG ne change pas son type.
        assert_eq!(mime_of(png), mime_of(ExportFormat::Png { scale: 1.0 }));
    }

    #[test]
    fn chaque_erreur_du_presse_papiers_a_sa_phrase() {
        let samples = [
            ClipboardError::Empty,
            ClipboardError::Unsupported("Wayland"),
            ClipboardError::Backend("selection refused".into()),
        ];
        let mut seen = [false; 3];
        for sample in &samples {
            let index = match sample {
                ClipboardError::Empty => 0,
                ClipboardError::Unsupported(_) => 1,
                ClipboardError::Backend(_) => 2,
            };
            seen[index] = true;
            check(sample, &clipboard_error_message(sample));
        }
        assert!(seen.iter().all(|&s| s), "une variante n'a pas d'exemple");
        // Le détail et l'affichage sont repris tels quels dans la phrase.
        let phrase = clipboard_error_message(&ClipboardError::Backend("selection refused".into()));
        assert!(phrase.contains("selection refused"), "{phrase}");
        let phrase = clipboard_error_message(&ClipboardError::Unsupported("Wayland"));
        assert!(phrase.contains("Wayland"), "{phrase}");
    }

    /// Une application dont le graphique est au premier plan, et sa vue déjà
    /// créée (une vue naît à la première peinture de la fenêtre).
    fn banc() -> (Session, StokesForgeApp, NodeId) {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.graph_views.insert(graph, GraphView::new());
        assert_eq!(app.floating_tabs.last(), Some(&Tab::Graph(graph)));
        (session, app, graph)
    }

    /// Une image de 900 × 700 avec `events`, textures écartées.
    fn entree(ctx: &egui::Context, events: Vec<egui::Event>, contenu: impl FnMut(&mut egui::Ui)) {
        image_de(ctx, egui::vec2(900.0, 700.0), events, contenu);
    }

    /// Une image avec `events` où l'application lit Ctrl+C pour `tab` ; rend
    /// les actions poussées et les événements qui restent à d'autres.
    fn peint(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        events: Vec<egui::Event>,
    ) -> (Vec<Action>, Vec<egui::Event>) {
        let mut actions = Vec::new();
        let mut restants = Vec::new();
        entree(ctx, events, |ui| {
            app.request_active_figure_copy(ui.ctx(), tab, &mut actions);
            restants = ui.input(|input| input.events.clone());
        });
        (actions, restants)
    }

    fn copie() -> Vec<egui::Event> {
        vec![egui::Event::Copy]
    }

    fn est_copie(events: &[egui::Event]) -> bool {
        events.iter().any(|e| matches!(e, egui::Event::Copy))
    }

    /// Le geste voulu : graphique actif, rien de sélectionné, Ctrl+C. Une
    /// seule action, avec la figure cadrée par la vue ; l'événement est retiré
    /// pour que la grille d'un classeur visible derrière ne le lise pas.
    #[test]
    fn ctrl_c_sur_un_graphique_actif_sans_selection_copie_la_figure() {
        let (session, mut app, graph) = banc();
        let ctx = contexte();
        let tab = Some(Tab::Graph(graph));

        // Témoin : sans Ctrl+C, rien ne part.
        let (actions, _) = peint(&mut app, &ctx, tab, Vec::new());
        assert!(actions.is_empty(), "{actions:?}");

        let (actions, restants) = peint(&mut app, &ctx, tab, copie());
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        let cadre = app
            .read_graph_view(graph, |v| v.framed_spec(&spec))
            .unwrap();
        assert_eq!(
            actions,
            vec![Action::CopyGraphFigure {
                spec: Box::new(cadre)
            }]
        );
        assert!(!est_copie(&restants), "l'événement doit être consommé");
        session.shutdown().unwrap();
    }

    /// Un objet sélectionné garde Ctrl+C : le copier-coller d'objets
    /// graphiques n'est pas défini par #149, et l'événement reste lisible.
    #[test]
    fn un_objet_selectionne_garde_ctrl_c() {
        let (session, mut app, graph) = banc();
        let ctx = contexte();
        let tab = Some(Tab::Graph(graph));
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        let series = stokesforge_plot::SeriesRef::new(0, 0);
        assert!(spec.series_at(series).is_some(), "témoin : la série existe");
        app.with_graph_view(graph, |view| view.select_series(Some(series), &spec));
        let (actions, restants) = peint(&mut app, &ctx, tab, copie());
        assert!(actions.is_empty(), "{actions:?}");
        assert!(est_copie(&restants), "l'événement doit rester");

        // Une fois désélectionnée, la figure part : c'est bien la sélection
        // qui décidait.
        app.with_graph_view(graph, |view| view.select_series(None, &spec));
        let (actions, _) = peint(&mut app, &ctx, tab, copie());
        assert_eq!(actions.len(), 1);
        session.shutdown().unwrap();
    }

    /// Un champ de saisie focalisé garde Ctrl+C : il copie son texte.
    #[test]
    fn un_champ_de_saisie_garde_ctrl_c() {
        let (session, mut app, graph) = banc();
        let ctx = contexte();
        let tab = Some(Tab::Graph(graph));
        let id = egui::Id::new("champ");
        let mut texte = String::from("abc");
        let mut peint_champ = |app: &mut StokesForgeApp, events: Vec<egui::Event>, focus: bool| {
            let mut actions = Vec::new();
            let mut restants = Vec::new();
            entree(&ctx, events, |ui| {
                ui.add(egui::TextEdit::singleline(&mut texte).id(id));
                if focus {
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                app.request_active_figure_copy(ui.ctx(), tab, &mut actions);
                restants = ui.input(|input| input.events.clone());
            });
            (actions, restants)
        };
        peint_champ(&mut app, Vec::new(), true);
        let (actions, restants) = peint_champ(&mut app, copie(), false);
        assert!(actions.is_empty(), "{actions:?}");
        assert!(est_copie(&restants), "l'événement doit rester au champ");
        session.shutdown().unwrap();
    }

    /// Un classeur au premier plan, ou aucune fenêtre : pas de figure.
    #[test]
    fn seul_un_graphique_actif_copie_sa_figure() {
        let (session, mut app, _graph) = banc();
        let ctx = contexte();
        let book = app.state.books()[0].0;
        let (actions, restants) = peint(&mut app, &ctx, Some(Tab::Book(book)), copie());
        assert!(actions.is_empty(), "{actions:?}");
        assert!(est_copie(&restants));
        let (actions, _) = peint(&mut app, &ctx, None, copie());
        assert!(actions.is_empty(), "{actions:?}");
        session.shutdown().unwrap();
    }

    /// La figure visée est celle de l'onglet actif, pas d'un autre graphique
    /// ouvert : Ctrl+C sur le second donne le spec du second.
    #[test]
    fn la_figure_copiee_est_celle_du_graphique_actif() {
        let (session, mut app, premier) = banc();
        app.state.dispatch(Action::AddGraph);
        let second = app.state.graphs().last().unwrap().0;
        assert_ne!(premier, second);
        app.graph_views.insert(second, GraphView::new());
        let ctx = contexte();
        let spec_de = |app: &StokesForgeApp, graph| {
            stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap()
        };
        let (actions, _) = peint(&mut app, &ctx, Some(Tab::Graph(second)), copie());
        assert_eq!(
            actions,
            vec![Action::CopyGraphFigure {
                spec: Box::new(spec_de(&app, second))
            }]
        );
        assert_ne!(spec_de(&app, premier), spec_de(&app, second), "témoin");
        session.shutdown().unwrap();
    }

    /// Cliquer sur la figure ne laisse aucun focus clavier à un widget du
    /// graphique : sans quoi Ctrl+C, juste après un clic sur la figure, serait
    /// pris pour une saisie et ne copierait jamais.
    #[test]
    fn cliquer_sur_la_figure_ne_prend_pas_le_focus_clavier() {
        let (session, graph) = graphe();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let ctx = contexte();
        let mut view = GraphView::new();
        let mut actions = Vec::new();
        let mut peindre = |ui: &mut egui::Ui| {
            graph_view_contents(ui, &mut view, state.snapshot(), graph, &mut actions);
        };
        image(&ctx, Vec::new(), &mut peindre);
        let centre = egui::pos2(400.0, 300.0);
        cliquer(&ctx, centre, egui::PointerButton::Primary, &mut peindre);
        assert_eq!(ctx.memory(|memory| memory.focused()), None);
        assert!(!keyboard_focus_outside_grid(&ctx));

        // Témoin : le test sait voir un focus quand il y en a un.
        let ailleurs = egui::Id::new("ailleurs");
        let mut champ = |ui: &mut egui::Ui| {
            ui.add(egui::TextEdit::singleline(&mut String::new()).id(ailleurs));
            ui.memory_mut(|memory| memory.request_focus(ailleurs));
        };
        image(&ctx, Vec::new(), &mut champ);
        image(&ctx, Vec::new(), &mut champ);
        assert!(keyboard_focus_outside_grid(&ctx));
        session.shutdown().unwrap();
    }
}
