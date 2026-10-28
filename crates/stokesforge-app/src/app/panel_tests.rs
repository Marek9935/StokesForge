//! Guards #100: the panels keep their view state while changing
//! presentation; the deferred callbacks read the root's publications.

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
fn collapsed_strips_center_the_icons_and_the_title_in_their_thickness() {
    for panel in [Panel::Project, Panel::Object, Panel::Script] {
        let session = Session::spawn(Project::new("Strips").unwrap(), None).unwrap();
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
                "{panel:?}: {icon:?} / {strip:?}"
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
            "title centered: {center:?} / {expected:?}"
        );
        session.shutdown().unwrap();
    }
}

/// #142: the expanded console's handles stay neighbors on the left, and the
/// collapsed strip separates from the workspace by the same stroke as PM/OM.
#[test]
fn the_console_groups_its_handles_on_the_left_and_delimits_the_top_of_its_strip() {
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
            assert!(icons[0].x < rect.left() + cell, "chevron on the left");
            if !collapsed {
                assert!(
                    (icons[1].x - icons[0].x - cell).abs() < 0.1,
                    "Detach right after the chevron: {icons:?}"
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
                assert!(stroke.width > 0.0, "visible separation");
                assert!(
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::LineSegment { points, stroke: painted }
                        if *painted == stroke
                        && (points[0].x - rect.left()).abs() < 0.1
                        && (points[1].x - rect.right()).abs() < 0.1
                        && (points[0].y - rect.top() - stroke.width / 2.0).abs() < 0.1
                        && (points[1].y - points[0].y).abs() < 0.1)
                    }),
                    "top stroke over the whole width of the strip"
                );
            }
            session.shutdown().unwrap();
        }
    }
}

#[test]
fn collapsing_and_expanding_by_click_keeps_the_custom_dimension() {
    for panel in [Panel::Project, Panel::Object, Panel::Script] {
        let session = Session::spawn(Project::new("Panels").unwrap(), None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        let app = RefCell::new(StokesForgeApp::new(state));
        panel.set_collapsed(&mut app.borrow_mut(), false);
        app.borrow_mut()
            .state
            .set_object_manager_open_for_test(true);
        let ctx = context();
        // A size chosen by the user, distinct from the default, is the
        // witness: simply restoring the default size is not enough.
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
            Panel::Script => assert_eq!(before.y, 310.0, "witness: custom height"),
            _ => assert_eq!(before.x, 370.0, "witness: custom width"),
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
            "{panel:?} collapses by its chevron"
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
            "{panel:?} expands by its chevron"
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
fn panel_headers_have_only_collapse_and_detach_or_reattach() {
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
                    .push(panel_header(ui, "Panel", placement, collapsed));
            });
            output.textures_delta.clear();
            output
        };
        paint(&ctx, Vec::new());
        let output = paint(&ctx, Vec::new());
        let icons = find_icon_centers(&output.shapes);
        assert_eq!(icons.len(), count, "no Close or Maximize icon");
        for (index, icon) in icons.into_iter().enumerate() {
            let before = actions.borrow().len();
            click_at(&ctx, icon, egui::Modifiers::NONE, paint);
            let detach = placement == PanelPlacement::Detached
                || (!collapsed && index == usize::from(placement != PanelPlacement::Left));
            let actions = actions.borrow();
            assert_eq!(
                actions[before..].iter().any(|action| action.detach),
                detach,
                "the side and the orientation determine the order of the buttons"
            );
            assert_eq!(
                actions[before..]
                    .iter()
                    .any(|action| action.toggle_collapse),
                !detach
            );
        }
        if collapsed {
            // The former Detach cell is now title: a
            // click here must trigger neither detaching nor expanding.
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
fn detaching_by_the_icon_then_reattaching_by_click_or_os_close() {
    for panel in [Panel::Project, Panel::Object, Panel::Script] {
        let session = Session::spawn(Project::new("Panels").unwrap(), None).unwrap();
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
        // The home-made button and the compositor's cross follow two distinct
        // input paths, which must end up in the same attached panel.
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
            assert!(
                panel.detached(&app.borrow()),
                "{panel:?}: the icon detaches"
            );
            let registered = paint_root(&ctx, Vec::new());
            let id = panel.viewport();
            let callback = registered.viewport_output[&id]
                .viewport_ui_cb
                .clone()
                .expect("deferred callback of the panel");
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
                    *icons.last().expect("Reattach icon"),
                    egui::Modifiers::NONE,
                    paint_child,
                );
            }
            assert!(
                panel.detached(&app.borrow()),
                "the request waits for the drain at the root"
            );
            let attached_again = paint_root(&ctx, Vec::new());
            assert!(
                !panel.detached(&app.borrow()),
                "{panel:?}: the drain reattaches"
            );
            assert!(
                !attached_again.viewport_output.contains_key(&id),
                "the panel no longer redeclares a viewport"
            );
            assert!(
                !panel.collapsed(&app.borrow()),
                "the detaching does not collapse the panel"
            );
        }
        session.shutdown().unwrap();
    }
}

#[test]
fn the_object_manager_callback_follows_the_active_graph_and_the_new_snapshot() {
    let mut project = Project::new("Panels").unwrap();
    let first = project
        .add_graph(
            project.root(),
            Some("First"),
            stokesforge_plot::PlotSpec::empty("First"),
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
    assert!(painted(&paint_child(false), "First"));
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
            "witness: the viewport sleeps as long as its publication does not change"
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
        "the change wakes the viewport"
    );
    output.textures_delta.clear();
    let changed = paint_child(false);
    assert!(
        painted(&changed, "Second"),
        "the old callback reads the new graph"
    );
    assert!(!painted(&changed, "First"));
    settle(&mut app, second);
    session
        .handle()
        .execute(stokesforge_commands::Command::Rename {
            id: second,
            name: "Updated".into(),
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
        painted(&paint_child(false), "Updated"),
        "the snapshot is republished"
    );
    let closed = paint_child(true);
    assert!(closed.viewport_output[&id]
        .commands
        .contains(&egui::ViewportCommand::CancelClose));
    app.drain_object_manager(&ctx, &mut Vec::new());
    assert!(!app.om_detached, "the OS close reattaches the panel");
    session.shutdown().unwrap();
}

#[test]
fn detached_input_returns_to_the_attached_console_after_an_os_close() {
    let session = Session::spawn(Project::new("Console").unwrap(), None).unwrap();
    let state = AppState::new(session.handle(), String::new()).unwrap();
    let mut app = StokesForgeApp::new(state);
    app.state.set_script_source("initial_source".into());
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
    assert!(painted(&output, "initial_source"));
    if app.state.script_available() {
        click_at(
            &ctx,
            find_text_center(&output.shapes, "initial_source"),
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
                egui::Event::Text("modified_source".into()),
            ],
        );
        // A publication can land between the viewport's edit and its
        // drain at the root: the old buffer must not erase the input.
        let mut publication = ctx.run_ui(input(Vec::new()), |ui| {
            app.show_detached_script_console(ui.ctx())
        });
        publication.textures_delta.clear();
        assert!(painted(&paint(&ctx, Vec::new()), "modified_source"));
        app.drain_script_console(&ctx, &mut Vec::new());
        assert_eq!(app.state.script_source(), "modified_source");
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
fn detaching_and_reattaching_the_console_keeps_the_python_job_and_its_output() {
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
    let source = "import time\ntime.sleep(0.15)\nprint('kept_output')";
    state.set_script_source(source.into());
    state.dispatch(Action::RunScript);
    assert!(state.script_running());
    let mut app = StokesForgeApp::new(state);
    app.detach_script_console();
    assert!(app.state.script_running(), "the engine stays at the root");
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
        "reattaching does not lose the job's channel"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while app.state.script_running() {
        assert!(
            std::time::Instant::now() < deadline,
            "the script must finish"
        );
        app.script_open = false;
        app.script_collapsed = true;
        app.poll_background_jobs(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let output = app.state.script_output().expect("result of the same job");
    assert_eq!(output.stdout, "kept_output\n");
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
        painted(&output, "kept_output"),
        "the reopened panel receives the output"
    );
    output.textures_delta.clear();
    session.shutdown().unwrap();
}

/// The old callback must read the new settings even if neither the graph nor the
/// snapshot changes. The injected storage does not touch the machine.
#[test]
fn settings_wake_and_change_the_detached_panel_like_the_anchored_panel() {
    use stokesforge_app_state::preferences::{ObjectManagerGroup, Preferences};
    let mut project = Project::new("Detached settings").unwrap();
    let mut spec = stokesforge_plot::PlotSpec::empty("Witness title");
    spec.panels
        .get_mut(0)
        .unwrap()
        .annotations
        .push(stokesforge_plot::AnnotationSpec::new(
            "Witness annotation",
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
    assert!(painted(&child(), &tr!("object-manager-objects")));
    for _ in 0..8 {
        publish(&mut app);
        child();
    }
    let idle = publish(&mut app);
    assert_eq!(
        idle.viewport_output[&id].repaint_delay,
        std::time::Duration::MAX,
        "witness: an unchanged panel sleeps"
    );
    app.state.dispatch(Action::SetObjectManagerGroupOpen {
        group: ObjectManagerGroup::Axes,
        open: true,
    });
    let group_changed = publish(&mut app);
    assert_eq!(
        group_changed.viewport_output[&id].repaint_delay,
        std::time::Duration::ZERO,
        "a collapse setting alone wakes the panel, even if the axes are still filtered"
    );
    app.state.dispatch(Action::SetObjectManagerShowScales(true));
    let output = publish(&mut app);
    assert_eq!(
        output.viewport_output[&id].repaint_delay,
        std::time::Duration::ZERO
    );
    child();
    let output = child();
    assert!(painted(&output, &tr!("object-manager-axes")));
    assert!(painted(&output, &tr!("plot-axis-bottom")));
    assert!(
        !painted(&output, "Witness title"),
        "linked title never listed"
    );
    // The anchored panel reads the same initial filters and collapses.
    app.om_detached = false;
    publish(&mut app);
    let anchored = publish(&mut app);
    assert!(painted(&anchored, &tr!("object-manager-axes")));
    assert!(painted(&anchored, &tr!("plot-axis-bottom")));
    app.state.dispatch(Action::ResetObjectManagerSettings);
    publish(&mut app);
    let reset = publish(&mut app);
    assert!(!painted(&reset, &tr!("object-manager-axes")));
    assert!(painted(&reset, &tr!("object-manager-objects")));
    app.om_detached = true;
    publish(&mut app);
    child();
    assert!(!painted(&child(), &tr!("object-manager-axes")));
    assert!(painted(&child(), &tr!("object-manager-objects")));
    // The reset does not erase the axes already expanded. The filter reveals them,
    // despite the new default axes_open=false, in both presentations.
    app.state.dispatch(Action::SetObjectManagerShowScales(true));
    publish(&mut app);
    child();
    assert!(painted(&child(), &tr!("plot-axis-bottom")));
    app.om_detached = false;
    publish(&mut app);
    assert!(painted(&publish(&mut app), &tr!("plot-axis-bottom")));
    session.shutdown().unwrap();
}
