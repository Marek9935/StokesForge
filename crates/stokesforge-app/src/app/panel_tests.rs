//! Garde-fous #100 : les panneaux gardent leur état de vue en changeant de
//! présentation ; les callbacks différés lisent les publications de la racine.

use crate::theme;
use std::cell::RefCell;

use super::project_manager::tests::{click_at, find_text_center};
use super::test_support::painted;
use super::window_chrome::tests::find_icon_centers;
use super::window_chrome::{panel_header, PanelPlacement, PANEL_STRIP_THICKNESS};
use super::*;
use egui::containers::panel::PanelState;
use stokesforge_core::Project;
use stokesforge_session::Session;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    ctx.set_fonts(stokesforge_render_egui::font_definitions());
    theme::install(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    egui_extras::install_image_loaders(&ctx);
    ctx
}

fn input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        events,
        ..Default::default()
    }
}

#[derive(Clone, Copy, Debug)]
enum Panel {
    Project,
    Object,
    Script,
}

impl Panel {
    fn placement(self) -> PanelPlacement {
        match self {
            Self::Project => PanelPlacement::Left,
            Self::Object => PanelPlacement::Right,
            Self::Script => PanelPlacement::Bottom,
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::Project => "stokesforge_pm",
            Self::Object => "stokesforge_object_manager",
            Self::Script => "stokesforge_script",
        }
    }
    fn collapsed_id(self) -> &'static str {
        match self {
            Self::Project => "stokesforge_pm_collapsed",
            Self::Object => "stokesforge_object_manager_collapsed",
            Self::Script => "stokesforge_script_collapsed",
        }
    }
    fn set_collapsed(self, app: &mut StokesForgeApp, collapsed: bool) {
        match self {
            Self::Project => app.pm_collapsed = collapsed,
            Self::Object => app.om_collapsed = collapsed,
            Self::Script => app.script_collapsed = collapsed,
        }
    }
    fn collapsed(self, app: &StokesForgeApp) -> bool {
        match self {
            Self::Project => app.pm_collapsed,
            Self::Object => app.om_collapsed,
            Self::Script => app.script_collapsed,
        }
    }
    fn detached(self, app: &StokesForgeApp) -> bool {
        match self {
            Self::Project => app.pm_detached,
            Self::Object => app.om_detached,
            Self::Script => app.script_detached,
        }
    }
    fn viewport(self) -> egui::ViewportId {
        egui::ViewportId::from_hash_of(match self {
            Self::Project => "stokesforge_project_manager_window",
            Self::Object => "stokesforge_object_manager_window",
            Self::Script => "stokesforge_script_window",
        })
    }
    fn paint(self, app: &mut StokesForgeApp, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut actions = Vec::new();
        match self {
            Self::Project => app.show_project_manager(
                ui,
                &ctx,
                None,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut actions,
            ),
            Self::Object => app.object_manager_panel(ui, None, &mut actions),
            Self::Script => app.script_panel(ui, &ctx, &mut actions),
        }
    }
}

#[test]
fn les_bandes_repliees_centrent_les_icones_et_le_titre_dans_leur_epaisseur() {
    for panel in [Panel::Project, Panel::Object, Panel::Script] {
        let session = Session::spawn(Project::new("Bandes").unwrap(), None).unwrap();
        let mut app = StokesForgeApp::new(AppState::new(session.handle(), String::new()).unwrap());
        panel.set_collapsed(&mut app, true);
        app.state.set_object_manager_open_for_test(true);
        let ctx = context();
        let mut output = ctx.run_ui(input(Vec::new()), |ui| panel.paint(&mut app, ui));
        output.textures_delta.clear();
        let strip = PanelState::load(&ctx, egui::Id::new(panel.collapsed_id()))
            .unwrap()
            .outer_rect;
        let axis = if matches!(panel, Panel::Script) { 1 } else { 0 };
        assert_eq!(strip.size()[axis], crate::icons::ICON_SIZE);
        let icons = find_icon_centers(&output.shapes);
        assert_eq!(icons.len(), 1);
        for icon in icons {
            assert!(
                (icon[axis] - strip.center()[axis]).abs() < 0.1,
                "{panel:?} : {icon:?} / {strip:?}"
            );
        }
        let title = output
            .shapes
            .iter()
            .find_map(|painted| match &painted.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .unwrap();
        let center = title.pos
            + egui::emath::Rot2::from_angle(title.angle) * title.galley.rect.center().to_vec2();
        let expected = if matches!(panel, Panel::Script) {
            strip.center() + egui::vec2(crate::icons::ICON_SIZE / 2.0, 0.0)
        } else {
            egui::pos2(
                strip.center().x,
                strip.center().y + crate::icons::ICON_SIZE / 2.0,
            )
        };
        assert!(
            center.distance(expected) < 0.1,
            "titre centré : {center:?} / {expected:?}"
        );
        session.shutdown().unwrap();
    }
}

/// #142 : les prises de console déployée restent voisines à gauche, et la
/// bande repliée se sépare de l'espace de travail par le même trait que PM/OM.
#[test]
fn la_console_groupe_ses_prises_a_gauche_et_delimite_le_haut_de_sa_bande() {
    for width in [320.0, 1200.0] {
        for collapsed in [true, false] {
            let session = Session::spawn(Project::new("Console").unwrap(), None).unwrap();
            let mut app =
                StokesForgeApp::new(AppState::new(session.handle(), String::new()).unwrap());
            app.script_collapsed = collapsed;
            let ctx = context();
            let paint = |app: &mut StokesForgeApp| {
                let mut raw = input(Vec::new());
                raw.screen_rect.as_mut().unwrap().max.x = width;
                let mut output = ctx.run_ui(raw, |ui| Panel::Script.paint(app, ui));
                output.textures_delta.clear();
                output
            };
            paint(&mut app);
            let output = paint(&mut app);
            let id = if collapsed {
                Panel::Script.collapsed_id()
            } else {
                Panel::Script.id()
            };
            let rect = PanelState::load(&ctx, egui::Id::new(id))
                .unwrap()
                .outer_rect;
            let icons = find_icon_centers(&output.shapes);
            assert_eq!(icons.len(), if collapsed { 1 } else { 2 });
            let cell = if collapsed {
                crate::icons::ICON_SIZE
            } else {
                super::window_chrome::HEADER_ACTION_WIDTH
            };
            assert!(icons[0].x < rect.left() + cell, "chevron à gauche");
            if !collapsed {
                assert!(
                    (icons[1].x - icons[0].x - cell).abs() < 0.1,
                    "Détacher immédiatement après le chevron : {icons:?}"
                );
                assert!((icons[0].y - icons[1].y).abs() < 0.1);
            }
            if collapsed {
                let stroke = ctx
                    .style_of(ctx.theme())
                    .visuals
                    .widgets
                    .noninteractive
                    .bg_stroke;
                assert!(stroke.width > 0.0, "séparation visible");
                assert!(
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::LineSegment { points, stroke: painted }
                        if *painted == stroke
                        && (points[0].x - rect.left()).abs() < 0.1
                        && (points[1].x - rect.right()).abs() < 0.1
                        && (points[0].y - rect.top() - stroke.width / 2.0).abs() < 0.1
                        && (points[1].y - points[0].y).abs() < 0.1)
                    }),
                    "trait supérieur sur toute la largeur de la bande"
                );
            }
            session.shutdown().unwrap();
        }
    }
}

#[test]
fn replier_et_deployer_par_clic_conserve_la_dimension_personnalisee() {
    for panel in [Panel::Project, Panel::Object, Panel::Script] {
        let session = Session::spawn(Project::new("Panneaux").unwrap(), None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let app = RefCell::new(StokesForgeApp::new(state));
        panel.set_collapsed(&mut app.borrow_mut(), false);
        app.borrow_mut()
            .state
            .set_object_manager_open_for_test(true);
        let ctx = context();
        // Une taille choisie par l'utilisateur, distincte du défaut, est le
        // témoin : restaurer simplement la taille par défaut ne suffit pas.
        let size = match panel {
            Panel::Script => egui::vec2(1200.0, 310.0),
            _ => egui::vec2(370.0, 800.0),
        };
        ctx.data_mut(|data| {
            data.insert_persisted(
                egui::Id::new(panel.id()),
                PanelState {
                    outer_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, size),
                },
            )
        });
        let paint = |ctx: &egui::Context, events| {
            let mut output = ctx.run_ui(input(events), |ui| panel.paint(&mut app.borrow_mut(), ui));
            output.textures_delta.clear();
            output
        };
        paint(&ctx, Vec::new());
        let expanded = paint(&ctx, Vec::new());
        let before = PanelState::load(&ctx, egui::Id::new(panel.id()))
            .unwrap()
            .size();
        match panel {
            Panel::Script => assert_eq!(before.y, 310.0, "témoin : hauteur personnalisée"),
            _ => assert_eq!(before.x, 370.0, "témoin : largeur personnalisée"),
        }
        click_at(
            &ctx,
            find_icon_centers(&expanded.shapes)
                [usize::from(panel.placement() == PanelPlacement::Left)],
            egui::Modifiers::NONE,
            paint,
        );
        assert!(
            panel.collapsed(&app.borrow()),
            "{panel:?} se replie par son chevron"
        );
        let collapsed = paint(&ctx, Vec::new());
        let strip = PanelState::load(&ctx, egui::Id::new(panel.collapsed_id()))
            .unwrap()
            .size();
        match panel {
            Panel::Script => assert!(strip.y < before.y / 2.0),
            _ => assert!((strip.x - PANEL_STRIP_THICKNESS).abs() <= 1.0),
        }
        assert_eq!(
            PanelState::load(&ctx, egui::Id::new(panel.id()))
                .unwrap()
                .size(),
            before
        );
        click_at(
            &ctx,
            find_icon_centers(&collapsed.shapes)[0],
            egui::Modifiers::NONE,
            paint,
        );
        assert!(
            !panel.collapsed(&app.borrow()),
            "{panel:?} se déploie par son chevron"
        );
        paint(&ctx, Vec::new());
        assert_eq!(
            PanelState::load(&ctx, egui::Id::new(panel.id()))
                .unwrap()
                .size(),
            before
        );
        session.shutdown().unwrap();
    }
}

#[test]
fn les_entetes_de_panneau_n_ont_que_replier_et_detacher_ou_rattacher() {
    for (placement, collapsed, count) in [
        (PanelPlacement::Left, false, 2),
        (PanelPlacement::Left, true, 1),
        (PanelPlacement::Right, false, 2),
        (PanelPlacement::Right, true, 1),
        (PanelPlacement::Bottom, false, 2),
        (PanelPlacement::Bottom, true, 1),
        (PanelPlacement::Detached, false, 1),
    ] {
        let ctx = context();
        let actions = RefCell::new(Vec::new());
        let paint = |ctx: &egui::Context, events| {
            let mut output = ctx.run_ui(input(events), |ui| {
                actions
                    .borrow_mut()
                    .push(panel_header(ui, "Panneau", placement, collapsed));
            });
            output.textures_delta.clear();
            output
        };
        paint(&ctx, Vec::new());
        let output = paint(&ctx, Vec::new());
        let icons = find_icon_centers(&output.shapes);
        assert_eq!(icons.len(), count, "aucune icône Fermer ou Maximiser");
        for (index, icon) in icons.into_iter().enumerate() {
            let before = actions.borrow().len();
            click_at(&ctx, icon, egui::Modifiers::NONE, paint);
            let detach = placement == PanelPlacement::Detached
                || (!collapsed && index == usize::from(placement != PanelPlacement::Left));
            let actions = actions.borrow();
            assert_eq!(
                actions[before..].iter().any(|action| action.detach),
                detach,
                "le côté et l'orientation déterminent l'ordre des boutons"
            );
            assert_eq!(
                actions[before..]
                    .iter()
                    .any(|action| action.toggle_collapse),
                !detach
            );
        }
        if collapsed {
            // L'ancienne cellule de Détacher est désormais du titre : un
            // clic ici ne doit déclencher ni détachement ni déploiement.
            let hidden = if placement == PanelPlacement::Bottom {
                find_icon_centers(&output.shapes)[0] + egui::vec2(crate::icons::ICON_SIZE, 0.0)
            } else {
                find_icon_centers(&output.shapes)[0] + egui::vec2(0.0, crate::icons::ICON_SIZE)
            };
            let before = actions.borrow().len();
            click_at(&ctx, hidden, egui::Modifiers::NONE, paint);
            assert!(actions.borrow()[before..]
                .iter()
                .all(|action| !action.detach && !action.toggle_collapse));
        }
        let actions = actions.borrow();
        assert_eq!(actions.iter().any(|action| action.detach), !collapsed);
        assert_eq!(
            actions.iter().any(|action| action.toggle_collapse),
            placement != PanelPlacement::Detached
        );
        assert!(actions
            .iter()
            .all(|action| !action.close && !action.maximize));
    }
}

#[test]
fn le_menu_view_masque_et_reaffiche_chaque_panneau() {
    for (panel, title) in [
        (Panel::Project, tr!("project-manager-title")),
        (Panel::Object, tr!("action-object-manager")),
        (Panel::Script, tr!("script-title")),
    ] {
        let session = Session::spawn(Project::new("Panneaux").unwrap(), None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let app = RefCell::new(StokesForgeApp::new(state));
        app.borrow_mut()
            .state
            .set_object_manager_open_for_test(true);
        let ctx = context();
        let paint = |ctx: &egui::Context, events| {
            let mut output = ctx.run_ui(input(events), |ui| {
                let mut app = app.borrow_mut();
                let mut actions = Vec::new();
                app.menu_bar(ui, ctx, &mut actions, None);
                // Le menu doit émettre la même Action qu'en production,
                // mais ce banc ne doit pas enregistrer de préférence réelle.
                for action in actions {
                    match action {
                        Action::SetObjectManagerOpen(open) => {
                            app.state.set_object_manager_open_for_test(open);
                        }
                        other => panic!("action inattendue du menu View : {other:?}"),
                    }
                }
                panel.paint(&mut app, ui);
            });
            output.textures_delta.clear();
            output
        };
        paint(&ctx, Vec::new());
        for expected in [false, true] {
            let output = paint(&ctx, Vec::new());
            click_at(
                &ctx,
                find_text_center(&output.shapes, &tr!("menu-view")),
                egui::Modifiers::NONE,
                paint,
            );
            let menu = paint(&ctx, Vec::new());
            click_at(
                &ctx,
                find_text_center(&menu.shapes, &tr!("menu-panels")),
                egui::Modifiers::NONE,
                paint,
            );
            let mut menu = paint(&ctx, Vec::new());
            // Le menu est peint après le panneau : chercher sa dernière
            // occurrence évite de cliquer le titre du panneau homonyme.
            let last_center = |output: &egui::FullOutput, label: &str| {
                output
                    .shapes
                    .iter()
                    .rev()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .expect("entrée du menu Panneaux")
            };
            let item = if matches!(panel, Panel::Object) {
                click_at(
                    &ctx,
                    last_center(&menu, &title),
                    egui::Modifiers::NONE,
                    paint,
                );
                menu = paint(&ctx, Vec::new());
                last_center(&menu, &tr!("action-show-content"))
            } else {
                last_center(&menu, &title)
            };
            click_at(&ctx, item, egui::Modifiers::NONE, paint);
            let visible = match panel {
                Panel::Project => app.borrow().pm_open,
                Panel::Object => app.borrow().state.object_manager_open(),
                Panel::Script => app.borrow().script_open,
            };
            assert_eq!(visible, expected, "{panel:?} : bascule via View");
            paint(
                &ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let without_menu = paint(&ctx, Vec::new());
            assert_eq!(
                painted(&without_menu, &title),
                expected,
                "{panel:?} : le contenu suit la visibilité du menu"
            );
        }
        session.shutdown().unwrap();
    }
}

fn child_input(id: egui::ViewportId, events: Vec<egui::Event>, close: bool) -> egui::RawInput {
    let mut input = input(events);
    input.viewport_id = id;
    input.viewports.insert(
        id,
        egui::ViewportInfo {
            parent: Some(egui::ViewportId::ROOT),
            events: if close {
                vec![egui::ViewportEvent::Close]
            } else {
                Vec::new()
            },
            ..Default::default()
        },
    );
    input
}

#[test]
fn detacher_par_l_icone_puis_rattacher_par_clic_ou_fermeture_os() {
    for panel in [Panel::Project, Panel::Object, Panel::Script] {
        let session = Session::spawn(Project::new("Panneaux").unwrap(), None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let app = RefCell::new(StokesForgeApp::new(state));
        app.borrow_mut()
            .state
            .set_object_manager_open_for_test(true);
        panel.set_collapsed(&mut app.borrow_mut(), false);
        let ctx = context();
        let paint_root = |ctx: &egui::Context, events| {
            let mut output = ctx.run_ui(input(events), |ui| panel.paint(&mut app.borrow_mut(), ui));
            output.textures_delta.clear();
            output
        };
        paint_root(&ctx, Vec::new());
        // Le bouton maison et la croix du compositeur suivent deux chemins
        // d'entrée distincts, qui doivent finir dans le même panneau attaché.
        for close_os in [false, true] {
            let attached = paint_root(&ctx, Vec::new());
            assert!(!panel.detached(&app.borrow()));
            let icons = find_icon_centers(&attached.shapes);
            click_at(
                &ctx,
                icons[usize::from(panel.placement() != PanelPlacement::Left)],
                egui::Modifiers::NONE,
                paint_root,
            );
            assert!(panel.detached(&app.borrow()), "{panel:?} : l'icône détache");
            let registered = paint_root(&ctx, Vec::new());
            let id = panel.viewport();
            let callback = registered.viewport_output[&id]
                .viewport_ui_cb
                .clone()
                .expect("callback différé du panneau");
            assert_eq!(
                registered.viewport_output[&id].builder.decorations,
                Some(false)
            );
            let paint_child = |ctx: &egui::Context, events| {
                let mut output = ctx.run_ui(child_input(id, events, false), |ui| callback(ui));
                output.textures_delta.clear();
                output
            };
            paint_child(&ctx, Vec::new());
            let detached = paint_child(&ctx, Vec::new());
            if close_os {
                let mut output = ctx.run_ui(child_input(id, Vec::new(), true), |ui| callback(ui));
                assert!(output.viewport_output[&id]
                    .commands
                    .contains(&egui::ViewportCommand::CancelClose));
                output.textures_delta.clear();
            } else {
                let icons = find_icon_centers(&detached.shapes);
                click_at(
                    &ctx,
                    *icons.last().expect("icône Rattacher"),
                    egui::Modifiers::NONE,
                    paint_child,
                );
            }
            assert!(
                panel.detached(&app.borrow()),
                "la requête attend le drainage à la racine"
            );
            let attached_again = paint_root(&ctx, Vec::new());
            assert!(
                !panel.detached(&app.borrow()),
                "{panel:?} : le drainage rattache"
            );
            assert!(
                !attached_again.viewport_output.contains_key(&id),
                "le panneau ne redéclare plus de viewport"
            );
            assert!(
                !panel.collapsed(&app.borrow()),
                "le détachement ne replie pas le panneau"
            );
        }
        session.shutdown().unwrap();
    }
}

#[test]
fn le_callback_object_manager_suit_le_graphe_actif_et_le_nouvel_instantane() {
    let mut project = Project::new("Panneaux").unwrap();
    let first = project
        .add_graph(
            project.root(),
            Some("Premier"),
            stokesforge_plot::PlotSpec::empty("Premier"),
        )
        .unwrap();
    let second = project
        .add_graph(
            project.root(),
            Some("Second"),
            stokesforge_plot::PlotSpec::empty("Second"),
        )
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new(session.handle(), String::new()).unwrap();
    let mut app = StokesForgeApp::new(state);
    app.state.set_object_manager_open_for_test(true);
    app.om_detached = true;
    let ctx = context();
    let id = egui::ViewportId::from_hash_of("stokesforge_object_manager_window");
    let mut output = ctx.run_ui(input(Vec::new()), |ui| {
        app.object_manager_panel(ui, Some(Tab::Graph(first)), &mut Vec::new())
    });
    let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
    output.textures_delta.clear();
    let paint_child = |close| {
        let mut output = ctx.run_ui(child_input(id, Vec::new(), close), |ui| callback(ui));
        output.textures_delta.clear();
        output
    };
    paint_child(false);
    assert!(painted(&paint_child(false), "Premier"));
    let settle = |app: &mut StokesForgeApp, graph| {
        for _ in 0..8 {
            ctx.run_ui(input(Vec::new()), |ui| {
                app.object_manager_panel(ui, Some(Tab::Graph(graph)), &mut Vec::new());
            })
            .textures_delta
            .clear();
            paint_child(false);
        }
        let mut idle = ctx.run_ui(input(Vec::new()), |ui| {
            app.object_manager_panel(ui, Some(Tab::Graph(graph)), &mut Vec::new());
        });
        assert_eq!(
            idle.viewport_output[&id].repaint_delay,
            std::time::Duration::MAX,
            "témoin : le viewport dort tant que sa publication ne change pas"
        );
        idle.textures_delta.clear();
    };
    settle(&mut app, first);
    let mut output = ctx.run_ui(input(Vec::new()), |ui| {
        app.object_manager_panel(ui, Some(Tab::Graph(second)), &mut Vec::new())
    });
    assert_eq!(
        output.viewport_output[&id].repaint_delay,
        std::time::Duration::ZERO,
        "le changement réveille le viewport"
    );
    output.textures_delta.clear();
    let changed = paint_child(false);
    assert!(
        painted(&changed, "Second"),
        "l'ancien callback lit le nouveau graphe"
    );
    assert!(!painted(&changed, "Premier"));
    settle(&mut app, second);
    session
        .handle()
        .execute(stokesforge_commands::Command::Rename {
            id: second,
            name: "Actualisé".into(),
        })
        .unwrap();
    app.state.refresh();
    let mut output = ctx.run_ui(input(Vec::new()), |ui| {
        app.object_manager_panel(ui, Some(Tab::Graph(second)), &mut Vec::new())
    });
    assert_eq!(
        output.viewport_output[&id].repaint_delay,
        std::time::Duration::ZERO
    );
    output.textures_delta.clear();
    assert!(
        painted(&paint_child(false), "Actualisé"),
        "l'instantané est republié"
    );
    let closed = paint_child(true);
    assert!(closed.viewport_output[&id]
        .commands
        .contains(&egui::ViewportCommand::CancelClose));
    app.drain_object_manager(&ctx, &mut Vec::new());
    assert!(!app.om_detached, "la fermeture OS rattache le panneau");
    session.shutdown().unwrap();
}

#[test]
fn la_saisie_detachee_revient_a_la_console_attachee_apres_fermeture_os() {
    let session = Session::spawn(Project::new("Console").unwrap(), None).unwrap();
    let state = AppState::new(session.handle(), String::new()).unwrap();
    let mut app = StokesForgeApp::new(state);
    app.state.set_script_source("source_initiale".into());
    app.detach_script_console();
    let ctx = context();
    let id = egui::ViewportId::from_hash_of("stokesforge_script_window");
    let mut output = ctx.run_ui(input(Vec::new()), |ui| {
        app.show_detached_script_console(ui.ctx())
    });
    let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
    output.textures_delta.clear();
    let paint = |ctx: &egui::Context, events| {
        let mut output = ctx.run_ui(child_input(id, events, false), |ui| callback(ui));
        output.textures_delta.clear();
        output
    };
    paint(&ctx, Vec::new());
    let output = paint(&ctx, Vec::new());
    assert!(painted(&output, "source_initiale"));
    if app.state.script_available() {
        click_at(
            &ctx,
            find_text_center(&output.shapes, "source_initiale"),
            egui::Modifiers::NONE,
            paint,
        );
        paint(
            &ctx,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::Text("source_modifiee".into()),
            ],
        );
        // Une publication peut tomber entre l'édition du viewport et son
        // drainage à la racine : l'ancien tampon ne doit pas effacer la saisie.
        let mut publication = ctx.run_ui(input(Vec::new()), |ui| {
            app.show_detached_script_console(ui.ctx())
        });
        publication.textures_delta.clear();
        assert!(painted(&paint(&ctx, Vec::new()), "source_modifiee"));
        app.drain_script_console(&ctx, &mut Vec::new());
        assert_eq!(app.state.script_source(), "source_modifiee");
    }
    let mut closed = ctx.run_ui(child_input(id, Vec::new(), true), |ui| callback(ui));
    assert!(closed.viewport_output[&id]
        .commands
        .contains(&egui::ViewportCommand::CancelClose));
    closed.textures_delta.clear();
    app.drain_script_console(&ctx, &mut Vec::new());
    assert!(!app.script_detached);
    app.script_collapsed = false;
    let mut attached = ctx.run_ui(input(Vec::new()), |ui| {
        app.script_panel(ui, &ctx, &mut Vec::new())
    });
    assert!(painted(&attached, app.state.script_source()));
    attached.textures_delta.clear();
    session.shutdown().unwrap();
}

#[cfg(feature = "python")]
#[test]
fn detacher_et_rattacher_la_console_conserve_le_travail_python_et_sa_sortie() {
    use stokesforge_python::interpreter::{self, EmbeddedPython};
    let session = Session::spawn(Project::new("Console").unwrap(), None).unwrap();
    let mut state = AppState::new(session.handle(), String::new()).unwrap();
    let home = std::env::var_os(interpreter::HOME_ENV)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../vendor/python/extracted/python")
        });
    state.set_python(EmbeddedPython::with_home(home));
    let source = "import time\ntime.sleep(0.15)\nprint('sortie_conservee')";
    state.set_script_source(source.into());
    state.dispatch(Action::RunScript);
    assert!(state.script_running());
    let mut app = StokesForgeApp::new(state);
    app.detach_script_console();
    assert!(app.state.script_running(), "le moteur reste à la racine");
    let ctx = context();
    let id = egui::ViewportId::from_hash_of("stokesforge_script_window");
    let mut output = ctx.run_ui(input(Vec::new()), |ui| {
        app.show_detached_script_console(ui.ctx())
    });
    let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
    output.textures_delta.clear();
    let mut output = ctx.run_ui(child_input(id, Vec::new(), true), |ui| callback(ui));
    output.textures_delta.clear();
    app.drain_script_console(&ctx, &mut Vec::new());
    assert!(!app.script_detached);
    assert!(
        app.state.script_running(),
        "rattacher ne perd pas le canal du travail"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while app.state.script_running() {
        assert!(
            std::time::Instant::now() < deadline,
            "le script doit terminer"
        );
        app.script_open = false;
        app.script_collapsed = true;
        app.poll_background_jobs(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let output = app.state.script_output().expect("résultat du même travail");
    assert_eq!(output.stdout, "sortie_conservee\n");
    assert!(output.error.is_none(), "{:?}", output.error);
    assert_eq!(app.state.script_source(), source);
    app.detach_script_console();
    let mut output = ctx.run_ui(input(Vec::new()), |ui| {
        app.show_detached_script_console(ui.ctx())
    });
    let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
    output.textures_delta.clear();
    let mut output = ctx.run_ui(child_input(id, Vec::new(), false), |ui| callback(ui));
    assert!(
        painted(&output, "sortie_conservee"),
        "le panneau rouvert reçoit la sortie"
    );
    output.textures_delta.clear();
    session.shutdown().unwrap();
}

/// L'ancien callback doit lire les nouveaux réglages même si ni graphe ni
/// instantané ne changent. Le stockage injecté ne touche pas la machine.
#[test]
fn les_reglages_reveillent_et_changent_le_panneau_detache_comme_le_panneau_ancre() {
    use crate::preferences::{ObjectManagerGroup, Preferences};
    let mut project = Project::new("Réglages détachés").unwrap();
    let mut spec = stokesforge_plot::PlotSpec::empty("Titre témoin");
    spec.panels
        .get_mut(0)
        .unwrap()
        .annotations
        .push(stokesforge_plot::AnnotationSpec::new(
            "Annotation témoin",
            0.0,
            0.0,
        ));
    let graph = project.add_graph(project.root(), None, spec).unwrap();
    let session = Session::spawn(project, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let preferences = Preferences::load_from(Some(dir.path().join("preferences.json")));
    let state =
        AppState::new_with_preferences(session.handle(), String::new(), preferences).unwrap();
    let mut app = StokesForgeApp::new(state);
    app.om_detached = true;
    let ctx = context();
    let id = egui::ViewportId::from_hash_of("stokesforge_object_manager_window");
    let publish = |app: &mut StokesForgeApp| {
        let mut output = ctx.run_ui(input(Vec::new()), |ui| {
            app.object_manager_panel(ui, Some(Tab::Graph(graph)), &mut Vec::new());
        });
        output.textures_delta.clear();
        output
    };
    let output = publish(&mut app);
    let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
    let child = || {
        let mut output = ctx.run_ui(child_input(id, Vec::new(), false), |ui| callback(ui));
        output.textures_delta.clear();
        output
    };
    child();
    assert!(!painted(&child(), &tr!("object-manager-axes")));
    assert!(painted(&child(), "Annotation témoin"));
    for _ in 0..8 {
        publish(&mut app);
        child();
    }
    let idle = publish(&mut app);
    assert_eq!(
        idle.viewport_output[&id].repaint_delay,
        std::time::Duration::MAX,
        "témoin : un panneau inchangé dort"
    );
    app.state.dispatch(Action::SetObjectManagerGroupOpen {
        group: ObjectManagerGroup::Axes,
        open: true,
    });
    let group_changed = publish(&mut app);
    assert_eq!(
        group_changed.viewport_output[&id].repaint_delay,
        std::time::Duration::ZERO,
        "un réglage de repli seul réveille le panneau, même si les axes sont encore filtrés"
    );
    app.state.dispatch(Action::SetObjectManagerShowScales(true));
    app.state.dispatch(Action::SetObjectManagerShowTexts(true));
    let output = publish(&mut app);
    assert_eq!(
        output.viewport_output[&id].repaint_delay,
        std::time::Duration::ZERO
    );
    child();
    let output = child();
    assert!(painted(&output, &tr!("object-manager-axes")));
    assert!(painted(&output, &tr!("plot-axis-bottom")));
    assert!(painted(&output, "Titre témoin"));
    // Le panneau ancré lit les mêmes filtres et replis initiaux.
    app.om_detached = false;
    publish(&mut app);
    let anchored = publish(&mut app);
    assert!(painted(&anchored, &tr!("object-manager-axes")));
    assert!(painted(&anchored, &tr!("plot-axis-bottom")));
    app.state.dispatch(Action::ResetObjectManagerSettings);
    publish(&mut app);
    let reset = publish(&mut app);
    assert!(!painted(&reset, &tr!("object-manager-axes")));
    assert!(painted(&reset, "Annotation témoin"));
    app.om_detached = true;
    publish(&mut app);
    child();
    assert!(!painted(&child(), &tr!("object-manager-axes")));
    assert!(painted(&child(), "Annotation témoin"));
    // Le reset n'efface pas les axes déjà dépliés. Le filtre les révèle,
    // malgré le nouveau défaut axes_open=false, dans les deux présentations.
    app.state.dispatch(Action::SetObjectManagerShowScales(true));
    publish(&mut app);
    child();
    assert!(painted(&child(), &tr!("plot-axis-bottom")));
    app.om_detached = false;
    publish(&mut app);
    assert!(painted(&publish(&mut app), &tr!("plot-axis-bottom")));
    session.shutdown().unwrap();
}
