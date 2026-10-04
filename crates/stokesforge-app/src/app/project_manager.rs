//! Arbre du Project Manager : une seule écriture pour le panneau fixe et la
//! fenêtre OS détachée, sur un arbre déjà construit
//! ([`crate::state::ProjectTreeNode`]). Ce que les clics demandent (ouvrir,
//! fermer, sélectionner, afficher, masquer) s'accumule dans
//! [`PmInteraction`], appliqué par `StokesForgeApp` après la peinture.
//!
//! Sorti d'`app.rs` tel quel (#121).

use std::collections::HashSet;

use eframe::egui;

use crate::icons::{icon_image, Icon, IconRole};
use crate::state::{ProjectTreeNode, WindowKind};
use crate::tr;

use super::tab::{tab_for_window, Tab};

/// Ce que le rendu de l'arbre du Project Manager accumule ou lit, à travers
/// [`project_manager_contents_from_tree`]/[`project_tree_node_ui`] — un seul
/// paquet plutôt que cinq paramètres, pour les deux mêmes contextes que le
/// reste du module (panneau fixe et fenêtre OS détachée, voir la doc de tête
/// de [`StokesForgeApp::show_detached_project_manager`]).
pub(super) struct PmInteraction<'a> {
    /// Onglet ouvert ou focalisé par un simple clic — voir
    /// [`StokesForgeApp::open_tab`].
    pub(super) open_requests: &'a mut Vec<Tab>,
    /// Onglet fermé par un double-clic sur une entrée déjà au premier plan,
    /// ou par « Masquer » sur une entrée maximisée (garde de cohérence, même
    /// esprit que celle de `detach_tab`).
    pub(super) close_requests: &'a mut Vec<Tab>,
    /// L'onglet actuellement au premier plan — voir la doc de tête du
    /// module, § z-order. `None` si aucune fenêtre flottante n'est ouverte.
    pub(super) active_tab: Option<Tab>,
    /// Sélection courante dans l'arbre — Ctrl/Cmd-clic en bascule
    /// l'appartenance sans ouvrir l'onglet.
    pub(super) selection: &'a mut HashSet<Tab>,
    /// Onglets actuellement masqués (voir [`StokesForgeApp::hidden_tabs`]) —
    /// affichés atténués dans l'arbre, et ce que « Afficher »/« Masquer »
    /// modifient pour la sélection courante.
    pub(super) hidden: &'a mut HashSet<Tab>,
}

/// Le corps du Project Manager, à partir d'un arbre déjà construit plutôt
/// que d'un `&AppState` vivant — seul le callback `show_viewport_deferred`
/// de [`StokesForgeApp::show_detached_project_manager`] a besoin de cette forme :
/// il ne porte qu'une copie de l'arbre ([`ProjectManagerViewModel`]), jamais
/// `AppState` elle-même. Le panneau fixe ([`StokesForgeApp::project_manager_panel`])
/// passe directement `&self.state.project_tree()` — même fonction, deux
/// sources d'arbre.
pub(super) fn project_manager_contents_from_tree(
    ui: &mut egui::Ui,
    tree: &[ProjectTreeNode],
    interaction: &mut PmInteraction,
) {
    ui.heading(tr!("project-manager-title"));
    ui.separator();
    if !interaction.selection.is_empty() {
        // Barre d'actions groupées, seulement quand elle a un effet — pas de
        // boutons désactivés à regarder tant que rien n'est sélectionné.
        ui.horizontal(|ui| {
            if ui
                .button(tr!("project-manager-show"))
                .on_hover_text(tr!("project-manager-show", "hint"))
                .clicked()
            {
                for &tab in interaction.selection.iter() {
                    interaction.hidden.remove(&tab);
                    interaction.open_requests.push(tab);
                }
            }
            if ui
                .button(tr!("project-manager-hide"))
                .on_hover_text(tr!("project-manager-hide", "hint"))
                .clicked()
            {
                for &tab in interaction.selection.iter() {
                    interaction.hidden.insert(tab);
                }
            }
            ui.weak(tr!(
                "project-manager-selected",
                count = interaction.selection.len()
            ));
        });
        ui.separator();
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        crate::tree_lines::begin_tree(ui);
        for node in tree {
            project_tree_node_ui(ui, node, interaction);
        }
    });
}

/// Peint récursivement un nœud de [`ProjectTreeNode`] et ses enfants ; rend
/// le centre vertical de sa rangée, pour les connecteurs de son parent.
fn project_tree_node_ui(
    ui: &mut egui::Ui,
    node: &ProjectTreeNode,
    interaction: &mut PmInteraction,
) -> f32 {
    match node {
        ProjectTreeNode::Folder { id, name, children } => {
            let state_id = ui.make_persistent_id(("project_tree_folder", id));
            let (toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                state_id,
                false,
            )
            .show_header(ui, |ui| {
                ui.add(icon_image(Icon::Folder, IconRole::Folder));
                ui.label(name);
            })
            .body(|ui| {
                crate::tree_lines::tree_children(ui, |ui, branch| {
                    if children.is_empty() {
                        branch.mark(ui.weak(tr!("project-folder-empty")).rect.center().y);
                    }
                    for child in children {
                        branch.mark(project_tree_node_ui(ui, child, interaction));
                    }
                });
            });
            toggle.rect.center().y
        }
        ProjectTreeNode::Window { id, name, kind } => {
            let icon = match kind {
                WindowKind::Book => (Icon::Workbook, IconRole::Workbook),
                WindowKind::Graph => (Icon::Graph, IconRole::Graph),
                WindowKind::Matrix => (Icon::Matrix, IconRole::Matrix),
                WindowKind::Note => (Icon::Note, IconRole::Note),
            };
            let tab = tab_for_window(*id, *kind);
            let selected = interaction.selection.contains(&tab);
            let is_hidden = interaction.hidden.contains(&tab);
            let fill = if selected {
                ui.visuals().selection.bg_fill
            } else {
                egui::Color32::TRANSPARENT
            };
            // Le bouton reste un `egui::Button` normal (affordance de survol
            // native) ; seul le fond du `Frame` qui l'entoure porte la
            // teinte de sélection, posée derrière lui. `push_id(tab, ...)`
            // est nécessaire : `Frame::show` fait naître son enfant avec un
            // salt fixe ("child"), identique à chaque itération de cette
            // boucle — sans un salt propre à `tab`, toutes les lignes de
            // l'arbre partageraient le même `Id` de bouton, et un clic
            // finirait attribué à la dernière ligne pressée plutôt qu'à
            // celle réellement cliquée.
            let response = ui
                .push_id(tab, |ui| {
                    egui::Frame::NONE
                        .fill(fill)
                        .inner_margin(2.0)
                        .show(ui, |ui| {
                            let text = if is_hidden {
                                egui::RichText::new(name).weak()
                            } else {
                                egui::RichText::new(name)
                            };
                            ui.add(egui::Button::image_and_text(
                                icon_image(icon.0, icon.1),
                                text,
                            ))
                        })
                        .inner
                })
                .inner;
            if response.clicked() {
                if ui.input(|input| input.modifiers.command) {
                    if !interaction.selection.remove(&tab) {
                        interaction.selection.insert(tab);
                    }
                } else {
                    interaction.open_requests.push(tab);
                }
            }
            if response.double_clicked() && interaction.active_tab == Some(tab) {
                interaction.close_requests.push(tab);
            }
            response.rect.center().y
        }
    }
}

/// `pub(super)` : [`tests::find_text_center`] et [`tests::click_at`] servent
/// aussi aux tests d'`app.rs` qui cliquent un texte peint.
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::cell::RefCell;

    use stokesforge_core::{NodeId, Project};
    use stokesforge_plot::PlotSpec;
    use stokesforge_session::Session;

    use crate::state::AppState;

    /// Trouve le centre du texte peint contenant `needle` dans `shapes` —
    /// utilisé pour cliquer une entrée du Project Manager sans supposer sa
    /// position exacte (mise en page auto), et par les tests d'`app.rs`. Même
    /// technique que
    /// `window_chrome::tests::len_tete_du_graphe_garde_son_titre_au_centre_et_sa_fleche_a_gauche`.
    pub(in crate::app) fn find_text_center(
        shapes: &[egui::epaint::ClippedShape],
        needle: &str,
    ) -> egui::Pos2 {
        shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle) => {
                    Some(shape.pos + shape.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("aucun texte contenant {needle:?} n'a été peint"))
    }

    /// Un clic (glissé nul, pression puis relâchement au même endroit) sur
    /// `pos`, réparti sur deux images comme le veut la détection de clic
    /// d'egui — voir
    /// `graph_window::tests::le_pan_principal_atteint_la_figure_dans_la_fenetre_flottante`
    /// pour le même schéma appliqué à un glissé.
    pub(in crate::app) fn click_at<R>(
        context: &egui::Context,
        pos: egui::Pos2,
        modifiers: egui::Modifiers,
        mut paint: impl FnMut(&egui::Context, Vec<egui::Event>) -> R,
    ) {
        // `PointerButton::modifiers` n'alimente pas `InputState::modifiers`
        // (voir `InputState::begin_pass`, qui ne le lit que sur
        // `Event::ModifiersChanged`/`Event::Key`) : sans cet événement
        // explicite, `ui.input(|i| i.modifiers.command)` resterait toujours
        // faux, quels que soient les `modifiers` posés sur les événements
        // de pointeur eux-mêmes.
        paint(
            context,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::PointerMoved(pos),
            ],
        );
        paint(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
        );
        paint(
            context,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
        if modifiers != egui::Modifiers::NONE {
            paint(
                context,
                vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
            );
        }
    }

    /// Construit l'arbre d'un projet Book+Graph, lu par `AppState` comme le
    /// panneau le lit — commun aux tests de double-clic et de sélection
    /// multiple du Project Manager.
    fn project_manager_fixture() -> (Session, Tab, Tab, Vec<ProjectTreeNode>) {
        let mut project = Project::new("Project Manager").unwrap();
        let root = project.root();
        project.add_empty_workbook(root, None).unwrap();
        let graph = project.add_graph(root, None, PlotSpec::default()).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), "test.stokesforge".to_string()).unwrap();
        let tree = state.project_tree();
        let book_id = tree
            .iter()
            .find_map(|node| match node {
                ProjectTreeNode::Window {
                    id,
                    kind: WindowKind::Book,
                    ..
                } => Some(*id),
                _ => None,
            })
            .expect("le projet du fixture porte un classeur");
        (session, Tab::Book(book_id), Tab::Graph(graph), tree)
    }

    /// Les accumulateurs de [`PmInteraction`] d'un test, et une image peinte
    /// de l'arbre sur un écran de 400 × 400.
    struct Banc {
        open_requests: RefCell<Vec<Tab>>,
        close_requests: RefCell<Vec<Tab>>,
        active_tab: Option<Tab>,
        selection: RefCell<HashSet<Tab>>,
        hidden: RefCell<HashSet<Tab>>,
    }

    impl Banc {
        fn new(active_tab: Option<Tab>, selection: HashSet<Tab>) -> Self {
            Banc {
                open_requests: RefCell::new(Vec::new()),
                close_requests: RefCell::new(Vec::new()),
                active_tab,
                selection: RefCell::new(selection),
                hidden: RefCell::new(HashSet::new()),
            }
        }

        fn paint(
            &self,
            context: &egui::Context,
            tree: &[ProjectTreeNode],
            events: Vec<egui::Event>,
        ) -> egui::FullOutput {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            let mut interaction = PmInteraction {
                open_requests: &mut self.open_requests.borrow_mut(),
                close_requests: &mut self.close_requests.borrow_mut(),
                active_tab: self.active_tab,
                selection: &mut self.selection.borrow_mut(),
                hidden: &mut self.hidden.borrow_mut(),
            };
            let mut output = context.run_ui(input, |ui| {
                project_manager_contents_from_tree(ui, tree, &mut interaction);
            });
            output.textures_delta.clear();
            output
        }
    }

    /// Un texte contenant `needle` a-t-il été peint ?
    fn painted(shapes: &[egui::epaint::ClippedShape], needle: &str) -> bool {
        shapes.iter().any(|painted| {
            matches!(&painted.shape,
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle))
        })
    }

    /// Couleur effective du premier texte peint contenant `needle`.
    fn text_color(shapes: &[egui::epaint::ClippedShape], needle: &str) -> egui::Color32 {
        shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(shape) if shape.galley.job.text.contains(needle) => {
                    let color = shape.galley.job.sections[0].format.color;
                    Some(if color == egui::Color32::PLACEHOLDER {
                        shape.fallback_color
                    } else {
                        color
                    })
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("aucun texte contenant {needle:?} n'a été peint"))
    }

    #[test]
    fn le_double_clic_ferme_lentree_deja_au_premier_plan_mais_pas_une_autre() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(Some(graph_tab), HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        let book_pos = find_text_center(&first.shapes, "Book");

        // Double-clic sur l'entrée déjà au premier plan (`active_tab` =
        // `graph_tab`) : doit fermer, pas seulement (re)focaliser.
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        assert!(
            banc.close_requests.borrow().contains(&graph_tab),
            "le double-clic sur l'onglet déjà au premier plan doit le fermer, close_requests={:?}",
            banc.close_requests.borrow()
        );

        // Double-clic sur une entrée qui n'est *pas* au premier plan
        // (`active_tab` reste `graph_tab`) : ouvre/focalise, ne ferme rien.
        // Contexte neuf : dans le même, egui compterait ces clics à la suite
        // des deux précédents (triple clic, puis davantage) et le geste ne
        // serait plus un double-clic — la vérification ne prouverait rien.
        let double_click_on_book = |active_tab| {
            let context = egui::Context::default();
            let banc = Banc::new(Some(active_tab), HashSet::new());
            let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);
            paint(&context, Vec::new());
            click_at(&context, book_pos, egui::Modifiers::NONE, paint);
            click_at(&context, book_pos, egui::Modifiers::NONE, paint);
            let open = banc.open_requests.borrow().clone();
            let close = banc.close_requests.borrow().clone();
            (open, close)
        };
        let (open, close) = double_click_on_book(graph_tab);
        assert!(
            !close.contains(&book_tab),
            "le double-clic sur une entrée qui n'est pas au premier plan ne doit pas la fermer"
        );
        assert!(
            open.contains(&book_tab),
            "le double-clic doit tout de même ouvrir/focaliser l'entrée, open_requests={open:?}"
        );
        // Témoin : le même geste, l'entrée étant au premier plan, la ferme —
        // c'est donc bien un double-clic qui a été envoyé.
        let (_, close) = double_click_on_book(book_tab);
        assert!(close.contains(&book_tab), "close_requests={close:?}");
    }

    /// Ctrl/Cmd-clic bascule l'appartenance à la sélection : il ajoute, puis
    /// retire au second clic, sans jamais ouvrir l'onglet.
    #[test]
    fn ctrl_clic_selectionne_sans_ouvrir() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(None, HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::COMMAND, paint);

        assert_eq!(
            *banc.selection.borrow(),
            HashSet::from([graph_tab]),
            "Ctrl/Cmd-clic doit ajouter l'entrée à la sélection"
        );
        assert!(
            banc.open_requests.borrow().is_empty(),
            "Ctrl/Cmd-clic ne doit jamais ouvrir l'onglet, open_requests={:?}",
            banc.open_requests.borrow()
        );

        // La sélection a fait apparaître la barre groupée au-dessus de
        // l'arbre : l'entrée a descendu, on la relit avant de recliquer.
        let with_toolbar = paint(&context, Vec::new());
        let graph_pos = find_text_center(&with_toolbar.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::COMMAND, paint);
        assert!(
            banc.selection.borrow().is_empty(),
            "un second Ctrl/Cmd-clic retire l'entrée de la sélection, selection={:?}",
            banc.selection.borrow()
        );
        assert!(banc.open_requests.borrow().is_empty());
    }

    #[test]
    fn masquer_et_afficher_agissent_sur_toute_la_selection() {
        let (_session, book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        // Sélection posée directement plutôt que reconstruite par deux
        // Ctrl-clics successifs (voir `ctrl_clic_selectionne_sans_ouvrir`
        // pour ce comportement-là, testé isolément) : ce test porte sur ce
        // que « Masquer »/« Afficher » font d'une sélection déjà en place,
        // pas sur la façon dont elle s'est constituée.
        let banc = Banc::new(None, HashSet::from([graph_tab, book_tab]));
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let with_toolbar = paint(&context, Vec::new());
        let hide_pos = find_text_center(&with_toolbar.shapes, &tr!("project-manager-hide"));
        click_at(&context, hide_pos, egui::Modifiers::NONE, paint);
        assert_eq!(
            *banc.hidden.borrow(),
            HashSet::from([graph_tab, book_tab]),
            "« Masquer » doit s'appliquer à toute la sélection, hidden={:?}",
            banc.hidden.borrow()
        );

        let with_hidden_rows = paint(&context, Vec::new());
        let show_pos = find_text_center(&with_hidden_rows.shapes, &tr!("project-manager-show"));
        click_at(&context, show_pos, egui::Modifiers::NONE, paint);
        assert!(
            banc.hidden.borrow().is_empty(),
            "« Afficher » doit vider le masquage de toute la sélection, hidden={:?}",
            banc.hidden.borrow()
        );
        assert!(
            banc.open_requests.borrow().contains(&graph_tab)
                && banc.open_requests.borrow().contains(&book_tab),
            "« Afficher » doit aussi garantir que chaque onglet est ouvert, open_requests={:?}",
            banc.open_requests.borrow()
        );
    }

    /// La barre « Afficher / Masquer » n'existe qu'avec une sélection ; sans
    /// elle, un simple clic ouvre l'entrée sans toucher à la sélection.
    #[test]
    fn la_barre_groupee_n_apparait_qu_avec_une_selection_et_un_clic_simple_ouvre() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(None, HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);

        let first = paint(&context, Vec::new());
        assert!(
            !painted(&first.shapes, &tr!("project-manager-hide"))
                && !painted(&first.shapes, &tr!("project-manager-show")),
            "sans sélection, aucun bouton groupé à regarder"
        );
        let graph_pos = find_text_center(&first.shapes, "Graph1");
        click_at(&context, graph_pos, egui::Modifiers::NONE, paint);
        assert_eq!(*banc.open_requests.borrow(), vec![graph_tab]);
        assert!(
            banc.selection.borrow().is_empty(),
            "un clic simple ne sélectionne pas"
        );
        assert!(banc.close_requests.borrow().is_empty());

        // Témoin : la même image, avec une sélection, peint bien la barre.
        banc.selection.borrow_mut().insert(graph_tab);
        let with_selection = paint(&context, Vec::new());
        assert!(painted(
            &with_selection.shapes,
            &tr!("project-manager-hide")
        ));
        assert!(painted(
            &with_selection.shapes,
            &tr!("project-manager-show")
        ));
    }

    /// Une entrée masquée reste listée, atténuée : sa couleur diffère de celle
    /// de la même entrée affichée, et ses voisines ne changent pas.
    #[test]
    fn une_entree_masquee_reste_listee_mais_attenuee() {
        let (_session, _book_tab, graph_tab, tree) = project_manager_fixture();
        let context = egui::Context::default();
        let banc = Banc::new(None, HashSet::new());
        banc.paint(&context, &tree, Vec::new());
        let shown = banc.paint(&context, &tree, Vec::new());
        banc.hidden.borrow_mut().insert(graph_tab);
        let hidden = banc.paint(&context, &tree, Vec::new());
        assert_ne!(
            text_color(&hidden.shapes, "Graph1"),
            text_color(&shown.shapes, "Graph1"),
            "l'entrée masquée doit se distinguer de l'entrée affichée"
        );
        assert_eq!(
            text_color(&hidden.shapes, "Book"),
            text_color(&shown.shapes, "Book"),
            "les autres entrées ne changent pas"
        );
    }

    /// Un dossier est replié par défaut : ses enfants ne sont pas peints.
    /// Déplié (clic sur son triangle), il les peint, et un dossier vide dit
    /// qu'il l'est au lieu de ne rien montrer.
    #[test]
    fn un_dossier_est_replie_par_defaut_et_un_dossier_vide_le_dit() {
        let tree = vec![
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(1),
                name: "Mesures".into(),
                children: vec![ProjectTreeNode::Window {
                    id: NodeId::from_raw(2),
                    name: "Spectre".into(),
                    kind: WindowKind::Graph,
                }],
            },
            ProjectTreeNode::Folder {
                id: NodeId::from_raw(3),
                name: "Vide".into(),
                children: Vec::new(),
            },
        ];
        let context = egui::Context::default();
        let banc = Banc::new(None, HashSet::new());
        let paint = |context: &egui::Context, events| banc.paint(context, &tree, events);
        paint(&context, Vec::new());
        let closed = paint(&context, Vec::new());
        assert!(painted(&closed.shapes, "Mesures") && painted(&closed.shapes, "Vide"));
        assert!(
            !painted(&closed.shapes, "Spectre"),
            "un dossier replié ne peint pas ses enfants"
        );
        assert!(!painted(&closed.shapes, &tr!("project-folder-empty")));
        assert!(
            banc.open_requests.borrow().is_empty(),
            "peindre n'ouvre rien"
        );

        // Le triangle d'un dossier est le premier élément de sa ligne, à
        // gauche de son icône : on clique à mi-hauteur de la ligne, au bord
        // gauche de l'arbre.
        let toggle = |shapes: &[egui::epaint::ClippedShape], name: &str| {
            let label = find_text_center(shapes, name);
            let left = shapes
                .iter()
                .map(|painted| painted.shape.visual_bounding_rect())
                .filter(|rect| rect.is_finite() && (rect.center().y - label.y).abs() < 4.0)
                .map(|rect| rect.min.x)
                .fold(f32::INFINITY, f32::min);
            egui::pos2(left + 4.0, label.y)
        };
        let at = toggle(&closed.shapes, "Mesures");
        click_at(&context, at, egui::Modifiers::NONE, paint);
        let opened = paint(&context, Vec::new());
        assert!(
            painted(&opened.shapes, "Spectre"),
            "le dossier déplié peint son enfant"
        );
        let at = toggle(&opened.shapes, "Vide");
        click_at(&context, at, egui::Modifiers::NONE, paint);
        let opened = paint(&context, Vec::new());
        assert!(painted(&opened.shapes, &tr!("project-folder-empty")));
        assert!(
            banc.open_requests.borrow().is_empty(),
            "déplier un dossier n'ouvre aucune fenêtre"
        );
    }
}
