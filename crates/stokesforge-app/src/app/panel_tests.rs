//! Guards #100: the panels keep their view state while changing
//! presentation; the deferred callbacks read the root's publications.

use crate::theme;
use std::cell::RefCell;

use super::project_manager::tests::click_at;
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
    fn paint(self, app: &mut StokesForgeApp, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut actions = Vec::new();
        match self {
            Self::Project => {
                app.show_project_manager(ui, &mut Vec::new(), &mut Vec::new(), &mut actions)
            }
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

/// The header icon detaches each panel: the root stops painting it (its own
/// window paints it, `detached_panel.rs`), the other presentation's state is
/// kept, and the detaching does not collapse it.
#[test]
fn detaching_by_the_icon_leaves_the_root_without_the_panel() {
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
        let after = paint_root(&ctx, Vec::new());
        assert!(
            find_icon_centers(&after.shapes).is_empty(),
            "{panel:?}: the root paints nothing of it"
        );
        assert!(
            !panel.collapsed(&app.borrow()),
            "the detaching does not collapse the panel"
        );
        session.shutdown().unwrap();
    }
}
