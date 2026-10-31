//! The images of the user manual, produced by a script (#365).
//!
//! # Why this exists
//!
//! The manual (`manual/src/`) shows the application. Screenshots taken by
//! hand go stale at the first change of the interface and differ from one
//! machine to the next. This bench renders the **real** application, headless
//! (`egui_kittest`, wgpu, `RendererOptions::PREDICTABLE`), on the demo project,
//! in English, with the light palette, at a fixed size and scale: the same
//! code gives the same pixels. Run it with `scripts/manual-screenshots.sh`.
//!
//! # Rules
//!
//! - One entry of [`SHOTS`] per image: a name (the PNG is
//!   `manual/src/images/<name>.png`), a setup that drives the application
//!   through `Action`s and labelled widgets (never by pixel coordinates) and
//!   what to crop. `manual/screenshots.json` lists every image, and a normal
//!   test keeps it equal to [`SHOTS`].
//! - Nothing here touches the user's profile: the configuration folder is a
//!   temporary one, and the `test-support` feature of the dev-dependency
//!   already isolates preferences and recovery copies.
//! - No mouse cursor, no tooltip, no caret: the pointer is sent away after
//!   every click, and the frames are run until the layout settles.
//! - A PNG is rewritten only if its decoded pixels differ from the file on
//!   disk, so an unchanged screen leaves the repository untouched.
//! - Only what the bench can render is illustrated: nothing that depends on
//!   the operating system (system file chooser, OS drag and drop, detached OS
//!   window) has a screenshot. The Python console needs the `python` feature
//!   (`"feature": "python"` in the manifest).

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use stokesforge_analysis::model::Model;
use stokesforge_app_state::analysis::Preset;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::object_manager::VisibilityTarget;
use stokesforge_app_state::preferences::Preferences;
use stokesforge_app_state::state::{Action, AppState, ImportStep, PlotAxis, PlotSettingsTab};
use stokesforge_core::compose::CellSide;
use stokesforge_core::{NodeId, NodeKind};
use stokesforge_plot::SeriesRef;
use stokesforge_session::Session;

use super::{StokesForgeApp, Tab};

/// The demo project, as `main.rs` builds it. Its files are included here
/// because the demo is a module of the binary, not of the library (the
/// binary-only `tests` of the demo are left out).
#[allow(dead_code)]
#[path = "../demo"]
mod demo {
    pub(super) mod analysis;
    pub(super) mod assemble;
    pub(super) mod graphs;
    pub(super) mod noise;
    pub(super) mod workbooks;

    pub(super) use assemble::demo_project;
    use assemble::set_x_role;
}

/// Logical size of the window, and scale: 2x gives crisp text on HiDPI
/// screens and stays readable when the page scales the image down.
const WINDOW: egui::Vec2 = egui::vec2(1440.0, 780.0);
const SCALE: f32 = 2.0;
/// Margin (logical points) kept around a cropped window or panel.
const MARGIN: f32 = 0.0;

/// What a shot keeps of the rendered window.
#[derive(Clone, Copy)]
enum Crop {
    /// The whole window.
    Whole,
    /// The rectangle (logical points) that the function finds, plus a margin.
    Rect(fn(&Harness<'static, Bench>) -> egui::Rect),
}

/// One image of the manual.
struct Shot {
    /// File name without extension, prefixed by the page it illustrates.
    name: &'static str,
    /// Brings the application to the state to show.
    setup: fn(&mut Harness<'static, Bench>),
    crop: Crop,
}

/// Rectangle (logical points) of the Project Manager panel.
fn project_manager_rect(h: &Harness<'static, Bench>) -> egui::Rect {
    // Down to the last row of the tree, not to the bottom of the panel.
    let panel = panel_rect(h, "stokesforge_pm");
    let last_row = h.get_by_label("Rows").rect();
    egui::Rect::from_min_max(panel.min, egui::pos2(panel.max.x, last_row.bottom() + 4.0))
}

/// Rectangle (logical points) of the Object Manager panel.
fn object_manager_rect(h: &Harness<'static, Bench>) -> egui::Rect {
    // The tree of Graph1 has four rows: the rest of the panel is empty.
    let panel = panel_rect(h, "stokesforge_object_manager");
    egui::Rect::from_min_size(panel.min, egui::vec2(panel.width(), 200.0))
}

fn panel_rect(h: &Harness<'static, Bench>, id: &str) -> egui::Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new(id))
        .unwrap_or_else(|| panic!("panel {id} is not placed"))
        .outer_rect
}

/// The rectangle of the topmost workspace window (a floating workbook or graph):
/// its frame, without the resize border and the shadow around it.
fn workspace_window_rect(h: &Harness<'static, Bench>) -> egui::Rect {
    layer_rect(h, egui::Order::Middle).shrink(super::window_chrome::RESIZE_BORDER)
}

/// The workbook window and the context menu open over it.
fn book_with_menu_rect(h: &Harness<'static, Bench>) -> egui::Rect {
    let screen = h.ctx.content_rect();
    let menus: Vec<egui::Rect> = h.ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.order == egui::Order::Foreground)
            .filter_map(|layer| memory.area_rect(layer.id))
            .filter(|rect| *rect != screen)
            .collect()
    });
    menus
        .into_iter()
        .fold(workspace_window_rect(h), |all, menu| all.union(menu))
}

/// The open menu (and its submenus): the popups above everything else.
fn menu_rect(h: &Harness<'static, Bench>) -> egui::Rect {
    let screen = h.ctx.content_rect();
    let menus: Vec<egui::Rect> = h.ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.order == egui::Order::Foreground)
            .filter_map(|layer| memory.area_rect(layer.id))
            .filter(|rect| *rect != screen)
            .collect()
    });
    menus
        .into_iter()
        .reduce(|all, menu| all.union(menu))
        .expect("an open menu")
}

/// The rectangle of the topmost dialog (modal windows live above the workspace):
/// exactly the window frame, stroke included. The area around it holds the
/// soft shadow and the resize border, where the application shows through.
fn dialog_rect(h: &Harness<'static, Bench>) -> egui::Rect {
    layer_rect(h, egui::Order::Foreground).shrink(super::window_chrome::RESIZE_BORDER)
}

/// Makes the open dialog `extra` points taller, so that its whole content fits
/// without its inner scroll bar, and settles the layout.
fn grow_dialog(h: &mut Harness<'static, Bench>, extra: f32) {
    let id = layer_id(h, egui::Order::Foreground);
    let size = dialog_rect(h).size() + egui::vec2(0.0, extra);
    super::dialogs::set_dialog_size(&h.ctx, id, size);
    settle(h);
}

fn layer_id(h: &Harness<'static, Bench>, order: egui::Order) -> egui::Id {
    let screen = h.ctx.content_rect();
    h.ctx.memory(|memory| {
        let found: Vec<egui::Id> = memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.order == order)
            .filter(|layer| memory.area_rect(layer.id).is_some_and(|r| r != screen))
            .map(|layer| layer.id)
            .collect();
        assert_eq!(found.len(), 1, "expected one visible {order:?} window");
        found[0]
    })
}

fn layer_rect(h: &Harness<'static, Bench>, order: egui::Order) -> egui::Rect {
    let screen = h.ctx.content_rect();
    h.ctx.memory(|memory| {
        // Closed windows keep their layer in memory: only the visible ones
        // count. The backdrop of a modal dialog covers the whole screen: it
        // is not the dialog.
        let found: Vec<egui::Rect> = memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.order == order)
            .filter_map(|layer| memory.area_rect(layer.id))
            .filter(|rect| *rect != screen)
            .collect();
        assert_eq!(
            found.len(),
            1,
            "expected one visible {order:?} window: {found:?}"
        );
        found[0]
    })
}

/// Every image rendered by the bench, in the order of the manifest.
const SHOTS: &[Shot] = &[
    Shot {
        name: "interface-main-window",
        setup: |h| {
            let (book, graph) = (h.state().ids.book1, h.state().ids.graph1);
            select_spectrum_sheet(h);
            place(
                h,
                Tab::Book(book),
                egui::pos2(4.0, 4.0),
                egui::vec2(380.0, 600.0),
            );
            place(
                h,
                Tab::Graph(graph),
                egui::pos2(388.0, 4.0),
                egui::vec2(440.0, 600.0),
            );
        },
        crop: Crop::Whole,
    },
    Shot {
        name: "interface-project-manager",
        setup: |_| {},
        crop: Crop::Rect(project_manager_rect),
    },
    Shot {
        name: "interface-object-manager",
        setup: |_| {},
        crop: Crop::Rect(object_manager_rect),
    },
    Shot {
        name: "workbooks-sheet",
        setup: show_book_alone,
        crop: Crop::Rect(workspace_window_rect),
    },
    Shot {
        name: "workbooks-column-menu",
        setup: |h| {
            show_book_alone(h);
            press_label(h, "B(Y)", egui::PointerButton::Secondary);
            settle(h);
        },
        crop: Crop::Rect(book_with_menu_rect),
    },
    Shot {
        name: "workbooks-formula-help",
        setup: formula_functions_open,
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "import-wizard-format",
        setup: |h| open_wizard_clean(h, ImportStep::Format),
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "import-wizard-header",
        setup: |h| open_wizard_clean(h, ImportStep::Header),
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "import-wizard-columns",
        setup: |h| open_wizard_clean(h, ImportStep::Columns),
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "graphs-properties-series",
        setup: |h| {
            empty_workspace(h);
            let graph = h.state().ids.graph1;
            act(
                h,
                Action::OpenObjectProperties {
                    graph,
                    target: VisibilityTarget::Series(SeriesRef { panel: 0, index: 0 }),
                },
            );
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "graphs-properties-general",
        setup: |h| {
            empty_workspace(h);
            let graph = h.state().ids.graph1;
            // Two layers side by side: the Size section lists two column
            // widths and one row height.
            act(
                h,
                Action::AddCell {
                    graph,
                    panel: 0,
                    side: CellSide::Right,
                    columns: Vec::new(),
                },
            );
            // Left stored (Top stays Auto): both kinds of value are shown.
            act(
                h,
                Action::SetFigureOffsets {
                    graph,
                    left: Some(100.0),
                    top: None,
                },
            );
            act(
                h,
                Action::OpenPlotSettings {
                    graph,
                    tab: PlotSettingsTab::Graph,
                    axis: None,
                },
            );
            grow_dialog(h, 250.0);
            // The Advanced block, closed by default, opened to show Figure size.
            click(h, "Advanced");
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "graphs-properties-axes",
        setup: |h| {
            empty_workspace(h);
            let graph = h.state().ids.graph1;
            act(
                h,
                Action::OpenPlotSettings {
                    graph,
                    tab: PlotSettingsTab::Axes,
                    axis: Some(PlotAxis::Bottom),
                },
            );
            grow_dialog(h, 255.0);
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "graphs-properties-overlay-axes",
        setup: |h| {
            empty_workspace(h);
            let graph = h.state().ids.graph1;
            // An overlay layer with its own X and Y axes, then its Top tab.
            act(
                h,
                Action::AddLayer {
                    graph,
                    panel: 0,
                    x_top: true,
                    y_right: true,
                },
            );
            // It plots the demo curve of Layer 1 against a titled top axis with
            // a typed range.
            if let Some(mut spec) = h.state().app.state.plot_spec(graph) {
                if let Some(series) = spec.panels.first().and_then(|p| p.series.first()).cloned() {
                    let layer = spec.panels.get_mut(1).expect("the overlay layer");
                    layer.series.push(series);
                    layer.bottom.label = "Wavelength".to_owned();
                    layer.bottom.unit = "nm".to_owned();
                    // The bounds shown come from a painted graph, which this
                    // shot does not have: typed bounds show real numbers.
                    layer.bottom.auto_range = false;
                    layer.bottom.range_min = Some(400.0);
                    layer.bottom.range_max = Some(700.0);
                }
                act(h, Action::SetPlotSpec { graph, spec });
            }
            act(
                h,
                Action::OpenPlotSettings {
                    graph,
                    tab: PlotSettingsTab::Axes,
                    axis: Some(PlotAxis::Top),
                },
            );
            if let Some(edit) = h.state_mut().app.state.plot_edit_mut() {
                edit.selected_panel = 1;
            }
            grow_dialog(h, 255.0);
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "graphs-formula-title",
        setup: |h| {
            let graph = node_named(h, "Annotations");
            show_only(h, Tab::Graph(graph), egui::vec2(760.0, 560.0));
        },
        crop: Crop::Rect(workspace_window_rect),
    },
    Shot {
        name: "graphs-layer-badges",
        setup: |h| {
            let graph = h.state().ids.graph1;
            // Two cells side by side: badge 1 and badge 2 sit at the top left
            // of their own cell.
            act(
                h,
                Action::AddCell {
                    graph,
                    panel: 0,
                    side: CellSide::Right,
                    columns: Vec::new(),
                },
            );
            show_only(h, Tab::Graph(graph), egui::vec2(760.0, 460.0));
        },
        crop: Crop::Rect(workspace_window_rect),
    },
    Shot {
        name: "analysis-integration",
        setup: |h| {
            open_analysis(h, Preset::Integrate, 0.0, 0.0);
            // One range around the main peak.
            if let Some(form) = h.state_mut().app.state.analysis_form_mut() {
                form.setup.integrate.ranges = vec![(470.0, 570.0)];
            }
            settle(h);
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "analysis-fit",
        setup: |h| {
            open_analysis(h, Preset::Fit(Model::Gaussian), 180.0, 50.0);
            click(h, "Auto");
            // Fitted on the main peak only.
            if let Some(form) = h.state_mut().app.state.analysis_form_mut() {
                form.setup.fit.range = Some((490.0, 550.0));
            }
            settle(h);
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "preferences-general",
        setup: |h| {
            empty_workspace(h);
            act(h, Action::OpenPreferences);
            grow_dialog(h, 100.0);
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "preferences-export",
        setup: |h| {
            empty_workspace(h);
            act(h, Action::OpenPreferences);
            click(h, "Export");
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "preferences-display-object-manager",
        setup: |h| {
            empty_workspace(h);
            act(h, Action::OpenPreferences);
            click(h, "Display");
            click(h, "Object Manager");
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "preferences-display-new",
        setup: |h| {
            empty_workspace(h);
            act(h, Action::OpenPreferences);
            click(h, "Display");
            click(h, "New");
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "preferences-folders",
        setup: |h| {
            empty_workspace(h);
            // A configuration folder at a fixed, readable place, holding the files
            // the application keeps there (the image must not change from run to run).
            let config = std::env::temp_dir()
                .join("stokesforge-manual")
                .join("StokesForge");
            let _ = std::fs::remove_dir_all(&config);
            std::fs::create_dir_all(&config).expect("configuration folder");
            for name in [
                "preferences.json",
                "couleurs.json",
                "import_recipes.json",
                "import_recent.json",
                "demo.stf",
            ] {
                std::fs::write(config.join(name), "{}").expect("file");
            }
            let handle = h.state()._session.handle();
            let mut state = AppState::new_with_preferences(
                handle,
                "Demo.stf".to_string(),
                Preferences::load_from(Some(config.join("preferences.json"))),
            )
            .expect("state");
            // Outside the configuration folder, as in the real application.
            state.set_recovery_dir(Some(
                std::env::temp_dir()
                    .join("stokesforge-manual")
                    .join("data")
                    .join("recovery"),
            ));
            h.state_mut().app.state = state;
            act(h, Action::OpenPreferences);
            click(h, "Folders");
            grow_dialog(h, 100.0);
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "files-recover-projects",
        setup: |h| {
            empty_workspace(h);
            let dir = h.state().scratch.path().join("recovery");
            std::fs::create_dir_all(&dir).expect("recovery folder");
            for (name, at) in [
                ("measurements-1.stf", 1_792_598_400),
                ("Untitled-2.stf", 1_792_512_000),
            ] {
                let path = dir.join(name);
                std::fs::write(&path, b"x").expect("copy");
                let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(at);
                std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .and_then(|file| file.set_modified(time))
                    .expect("date of the copy");
            }
            let state = &mut h.state_mut().app.state;
            state.set_recovery_dir(Some(dir));
            state.scan_recovery();
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "graphs-graph-menu",
        setup: |h| {
            let graph = h.state().ids.graph1;
            show_only(h, Tab::Graph(graph), egui::vec2(440.0, 600.0));
            press_label(h, "Graph", egui::PointerButton::Primary);
        },
        crop: Crop::Rect(menu_rect),
    },
    Shot {
        name: "graphs-axes-log-scale",
        setup: |h| {
            empty_workspace(h);
            let graph = h.state().ids.graph1;
            act(
                h,
                Action::OpenPlotSettings {
                    graph,
                    tab: PlotSettingsTab::Axes,
                    axis: Some(PlotAxis::Bottom),
                },
            );
            grow_dialog(h, 255.0);
            click(h, "Log scale");
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "analysis-baseline",
        setup: |h| {
            open_analysis(h, Preset::BaselinePolynomial, 0.0, 0.0);
            // Two zones with only background, on each side of the peaks.
            if let Some(form) = h.state_mut().app.state.analysis_form_mut() {
                form.setup.baseline.zones = vec![(400.0, 440.0), (650.0, 700.0)];
            }
            settle(h);
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "import-wizard-files",
        setup: |h| open_wizard_clean(h, ImportStep::Files),
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "import-wizard-metadata",
        setup: |h| open_wizard_clean(h, ImportStep::Metadata),
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "import-wizard-destination",
        setup: |h| open_wizard_clean(h, ImportStep::Destination),
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "interface-object-manager-series",
        setup: |h| {
            // The legend and the Object Manager both carry the name: the
            // Object Manager row is the last one.
            let row = h
                .get_all_by_label_contains("Sample")
                .last()
                .expect("series row")
                .rect()
                .center();
            h.event(egui::Event::PointerMoved(row));
            h.step();
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos: row,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
            }
            settle(h);
        },
        crop: Crop::Rect(object_manager_rect),
    },
    Shot {
        name: "analysis-peaks",
        setup: |h| {
            open_analysis(h, Preset::Peaks, 320.0, 120.0);
            settle(h);
        },
        crop: Crop::Rect(dialog_rect),
    },
    Shot {
        name: "workbooks-workbook-menu",
        setup: |h| {
            show_book_alone(h);
            // The menu follows the active window: bring the workbook forward.
            let book = Tab::Book(h.state().ids.book1);
            h.state_mut().app.open_tab(book);
            settle(h);
            press_label(h, "Workbook", egui::PointerButton::Primary);
        },
        crop: Crop::Rect(menu_rect),
    },
    // The Python console needs the embedded interpreter: only with the
    // `python` feature (scripts/manual-screenshots.sh runs both builds).
    #[cfg(feature = "python")]
    Shot {
        name: "scripting-console",
        setup: |h| {
            empty_workspace(h);
            h.state_mut().app.script_collapsed = false;
            let script = SCRIPT_EXAMPLE.to_string();
            h.state_mut().app.state.set_script_source(script);
            act(h, Action::RunScript);
            // The script runs on its own thread: wait for its output.
            for _ in 0..400 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                h.step();
                let state = &h.state().app.state;
                if !state.script_running() && state.script_output().is_some() {
                    break;
                }
            }
            settle(h);
        },
        crop: Crop::Rect(script_console_rect),
    },
];

/// The script shown in the console shot; it is really run.
#[cfg(feature = "python")]
const SCRIPT_EXAMPLE: &str = "\
import numpy as np

x = pyst.column(pyst.resolve(\"[Book1]Spectrum!A\"))
y = pyst.column(pyst.resolve(\"[Book1]Spectrum!B\"))
peak = int(np.argmax(y))
print(\"Maximum:\", round(float(y[peak]), 3), \"at\", float(x[peak]), \"nm\")
";

/// The script console panel at the bottom of the window, 1000 points wide.
#[cfg(feature = "python")]
fn script_console_rect(h: &Harness<'static, Bench>) -> egui::Rect {
    let panel = panel_rect(h, "stokesforge_script");
    egui::Rect::from_min_size(panel.min, egui::vec2(1000.0, panel.height()))
}

/// No floating window behind a dialog: its crop shows plain background.
fn empty_workspace(h: &mut Harness<'static, Bench>) {
    h.state_mut().app.floating_tabs.clear();
    settle(h);
}

/// Dispatches an action, as a menu entry would.
fn act(h: &mut Harness<'static, Bench>, action: Action) {
    h.state_mut().app.state.dispatch(action);
    settle(h);
}

/// Presses and releases `button` on the centre of the node with this label.
///
/// A click in the grid gives the keyboard focus to the grid's anchor
/// (`grid::claim_grid_focus`), whose AccessKit node only exists from the next
/// frame: for that one frame `accesskit_consumer` panics ("Focused ID is not
/// in the node list") after egui has finished the pass. The panic is caught
/// and the following frames, where the node exists, are consistent. Real
/// pointer events are used (the position comes from the labelled node, never
/// from a constant).
fn press_label(h: &mut Harness<'static, Bench>, label: &str, button: egui::PointerButton) {
    use std::panic::{catch_unwind, set_hook, take_hook, AssertUnwindSafe};
    let pos = h.get_by_label(label).rect().center();
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    let previous_hook = take_hook();
    set_hook(Box::new(|_| {}));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = catch_unwind(AssertUnwindSafe(|| h.step()));
    }
    set_hook(previous_hook);
    settle(h);
}

/// Clicks the widget with this label, then sends the pointer away.
fn click(h: &mut Harness<'static, Bench>, label: &str) {
    h.get_by_label(label).click();
    settle(h);
}

/// The node of the demo with this (unique) name.
fn node_named(h: &Harness<'static, Bench>, name: &str) -> NodeId {
    let tree = &h.state().app.state.snapshot().tree;
    tree.walk_all()
        .into_iter()
        .find(|id| tree.node(*id).is_some_and(|node| node.name() == name))
        .unwrap_or_else(|| panic!("{name} is not in the demo"))
}

/// Only this window in the workspace, at the top left.
fn show_only(h: &mut Harness<'static, Bench>, tab: Tab, size: egui::Vec2) {
    h.state_mut().app.floating_tabs.clear();
    h.state_mut().app.open_tab(tab);
    place(h, tab, egui::pos2(4.0, 4.0), size);
}

/// A small instrument file in the scratch folder, and the import wizard open
/// on it at `step` (the file chooser is bypassed: the paths are given).
fn open_wizard_clean(h: &mut Harness<'static, Bench>, step: ImportStep) {
    empty_workspace(h);
    open_wizard(h, step);
}

fn open_wizard(h: &mut Harness<'static, Bench>, step: ImportStep) {
    let file = h.state().scratch.path().join("spectrum.csv");
    let mut text =
        String::from("Instrument: Demo spectrometer\nSample: S-12\nWavelength;Absorbance\nnm;AU\n");
    for i in 0..12 {
        let wavelength = 400 + 25 * i;
        let absorbance = 0.05 + 0.8 * (-((f64::from(wavelength) - 520.0) / 60.0).powi(2)).exp();
        text.push_str(&format!("{wavelength};{absorbance:.3}\n").replace('.', ","));
    }
    std::fs::write(&file, text).expect("csv");
    let book = h.state().ids.book1;
    let state = &mut h.state_mut().app.state;
    state.open_import(Some(book));
    state.set_import_paths(vec![file]);
    if let Some(import) = state.import_state_mut() {
        import.recipe.separator = stokesforge_import::Separator::Semicolon;
        import.recipe.decimal = stokesforge_import::DecimalStyle::Comma;
    }
    state.auto_detect_import_header();
    if let Some(import) = state.import_state_mut() {
        // Sub-header: line 1 holds the long names, line 2 the units.
        let roles = &mut import.recipe.subheader_roles;
        roles.long_name = Some(stokesforge_import::SubheaderLineRange::single(1));
        roles.unit = Some(stokesforge_import::SubheaderLineRange::single(2));
    }
    settle(h);
    if let Some(import) = h.state_mut().app.state.import_state_mut() {
        import.step = step;
    }
}

/// The analysis window open on the series of `Graph1`.
///
/// `values` is the height (points) of the list of values under the preview, so
/// that it shows every value without its scroll bar (`0`: the application's
/// own height, enough for a short list); `grow` makes the window taller by as
/// much, to keep the preview and to show the whole settings column.
fn open_analysis(h: &mut Harness<'static, Bench>, preset: Preset, values: f32, grow: f32) {
    super::analysis_dialog::VALUES_HEIGHT_OVERRIDE
        .store(values as u32, std::sync::atomic::Ordering::Relaxed);
    empty_workspace(h);
    let graph = h.state().ids.graph1;
    let tree = &h.state().app.state.snapshot().tree;
    let spec = match tree.node(graph).map(|node| node.kind()) {
        Some(NodeKind::Graph { spec, .. }) => spec.clone(),
        _ => panic!("Graph1 is not a graph"),
    };
    act(
        h,
        Action::OpenGraphAnalysis {
            preset,
            graph,
            series: vec![SeriesRef { panel: 0, index: 0 }],
            spec: Box::new(spec),
        },
    );
    // Tall enough for the whole settings column and the values.
    if grow > 0.0 {
        grow_dialog(h, grow);
    }
}

/// Puts a floating window at `pos`, of `size` (coordinates of the workspace,
/// logical points). Takes effect at the next frame.
fn place(h: &mut Harness<'static, Bench>, tab: Tab, pos: egui::Pos2, size: egui::Vec2) {
    let app = &mut h.state_mut().app;
    app.window_positions.insert(tab, pos);
    app.window_sizes.insert(tab, size);
}

/// The "Spectrum" sheet of `Book1`: the data that `Graph1` plots.
fn select_spectrum_sheet(h: &mut Harness<'static, Bench>) {
    let sheet = node_named(h, "Spectrum");
    act(h, Action::SelectSheet(sheet));
    // Wide enough for the values (a width is not a command: no status message).
    let columns: Vec<NodeId> = h
        .state()
        .app
        .state
        .snapshot()
        .tree
        .children(sheet)
        .expect("sheet")
        .to_vec();
    for column in &columns[..2] {
        let column = *column;
        h.state_mut().app.state.dispatch(Action::SetColumnWidth {
            column,
            width: 96.0,
        });
    }
    settle(h);
}

/// Workbook `Book1` alone, large, in front.
fn show_book_alone(h: &mut Harness<'static, Bench>) {
    select_spectrum_sheet(h);
    let book = Tab::Book(h.state().ids.book1);
    h.state_mut().app.floating_tabs.retain(|tab| *tab == book);
    place(h, book, egui::pos2(4.0, 4.0), egui::vec2(560.0, 440.0));
}

/// The Formula Functions window (#402) on its "Column values" tab, opened by the
/// "?" of the `F(x)=` cell.
fn formula_functions_open(h: &mut Harness<'static, Bench>) {
    let family = stokesforge_app_state::formula_help::groups()
        .iter()
        .position(|group| {
            group
                .rows
                .iter()
                .any(|row| matches!(row.callee, Some(stokesforge_core::Callee::Column(_))))
        })
        .expect("a family of column functions");
    let state = &mut h.state_mut().app.state;
    state.dispatch(Action::OpenFormulaFunctions);
    state.dispatch(Action::SelectFormulaFunctionsFamily(family));
    settle(h);
}

/// The application and what keeps it alive.
struct Bench {
    app: StokesForgeApp,
    frame: eframe::Frame,
    _session: Session,
    ids: Ids,
    /// The recovery folder and the files of the import shots.
    scratch: tempfile::TempDir,
}

/// The nodes of the demo that the shots open.
struct Ids {
    book1: NodeId,
    graph1: NodeId,
}

impl Bench {
    fn new(spectrum_noise: f64) -> Self {
        let mut project = demo::demo_project().expect("demo project");
        // The demo leaves the long names and units of the "Spectrum" sheet
        // empty: fill them before the session owns the project (invariant 1).
        let spectrum = project
            .tree()
            .iter()
            .find_map(|(id, node)| (node.name() == "Spectrum").then_some(id))
            .expect("Spectrum sheet");
        let columns = project.tree().children(spectrum).expect("sheet").to_vec();
        for (column, name, unit) in [
            (columns[0], "Wavelength", "nm"),
            (columns[1], "Absorbance", "AU"),
        ] {
            let mut meta = project
                .tree()
                .node(column)
                .and_then(|node| node.as_column())
                .map(|column| column.meta().clone())
                .unwrap_or_default();
            meta.long_name = name.into();
            meta.unit = unit.into();
            project.set_column_meta(column, meta).expect("column meta");
        }
        if spectrum_noise > 0.0 {
            // A fixed, reproducible scatter (a measured spectrum is never exact).
            let mut state = 0x9E37_79B9_7F4A_7C15_u64;
            let mut uniform = move || {
                // xorshift64*: the same sequence on every run.
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1_u64 << 53) as f64
            };
            let noisy: Vec<f64> = (0..121_u32)
                .map(|i| {
                    let x = 400.0 + f64::from(i) * 2.5;
                    let (a, b) = ((x - 520.0) / 38.0, (x - 610.0) / 22.0);
                    let gauss: f64 = (0..12).map(|_| uniform()).sum::<f64>() - 6.0;
                    0.85 * (-a * a).exp() + 0.32 * (-b * b).exp() + 0.04 + gauss * spectrum_noise
                })
                .collect();
            project
                .set_values(columns[1], 0, stokesforge_core::ValueBlock::Float(noisy))
                .expect("noisy spectrum");
        }
        let node = |name: &str| {
            project
                .tree()
                .iter()
                .find_map(|(id, node)| (node.name() == name).then_some(id))
                .unwrap_or_else(|| panic!("{name} is not in the demo"))
        };
        let (book1, graph1) = (node("Book1"), node("Graph1"));
        let session = Session::spawn(project, None).expect("session");
        // The formula columns of the demo carry only their formula until a
        // full recalculation (invariant 11).
        session.handle().recalculate(&[]).expect("recalculation");
        #[cfg_attr(not(feature = "python"), allow(unused_mut))]
        let mut state = AppState::new_with_preferences(
            session.handle(),
            "Demo.stf".to_string(),
            Preferences::load_from(None),
        )
        .expect("state");
        #[cfg(feature = "python")]
        state.set_python(
            stokesforge_python::interpreter::EmbeddedPython::locate()
                .expect("embedded Python (STOKESFORGE_PYTHON_HOME, see scripts/python-test.sh)"),
        );
        let mut app = StokesForgeApp::new(state);
        app.open_tab(Tab::Book(book1));
        app.open_tab(Tab::Graph(graph1));
        Self {
            app,
            frame: eframe::Frame::_new_kittest(),
            _session: session,
            ids: Ids { book1, graph1 },
            scratch: tempfile::tempdir().expect("temporary folder"),
        }
    }
}

/// Folder of the generated images: `manual/src/images/`.
fn images_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manual/src/images")
}

/// Only one capture engine at a time (see `preferences_proto::tests`).
fn capture_lock() -> std::sync::MutexGuard<'static, ()> {
    static CAPTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    CAPTURES.lock().unwrap_or_else(|e| e.into_inner())
}

/// A harness on the real application, settled.
fn harness(spectrum_noise: f64) -> Harness<'static, Bench> {
    use eframe::App as _;
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .with_pixels_per_point(SCALE)
        .with_theme(egui::Theme::Light)
        .build_ui_state(
            |ui, bench: &mut Bench| {
                bench.app.logic(&ui.ctx().clone(), &mut bench.frame);
                bench.app.ui(ui, &mut bench.frame);
            },
            Bench::new(spectrum_noise),
        );
    harness
        .ctx
        .set_fonts(stokesforge_render_egui::font_definitions());
    egui_extras::install_image_loaders(&harness.ctx);
    crate::theme::install(&harness.ctx);
    settle(&mut harness);
    harness
}

/// Runs frames until the layout, the animations and the loaders are quiet.
fn settle(harness: &mut Harness<'static, Bench>) {
    harness.run_steps(3);
    harness.try_run_realtime().ok();
    // Background jobs (file previews, analysis previews) answer in real time.
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(15));
        harness.step();
    }
}

/// Sends the pointer away, so that no cursor, hover or tooltip is painted.
fn pointer_away(harness: &mut Harness<'static, Bench>) {
    harness.event(egui::Event::PointerGone);
    settle(harness);
}

/// Takes the keyboard focus away from every widget: no focus ring, no
/// selected text in a field that a window focused when it opened.
fn drop_focus(harness: &mut Harness<'static, Bench>) {
    harness.ctx.memory_mut(|memory| {
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
    });
    settle(harness);
}

/// The final image of a shot: setup, settle, render, crop.
fn render_shot(shot: &Shot) -> image::RgbaImage {
    // The fits need a measured-looking spectrum: the exact demo curve gives a
    // perfect fit, 17-digit values and residuals that only show a neighbour's tail.
    let noise = if matches!(shot.name, "analysis-peaks" | "analysis-fit") {
        0.006
    } else {
        0.0
    };
    let mut harness = harness(noise);
    (shot.setup)(&mut harness);
    pointer_away(&mut harness);
    drop_focus(&mut harness);
    let full = harness.render().expect("render");
    match shot.crop {
        Crop::Whole => full,
        Crop::Rect(find) => {
            let rect = find(&harness).expand(MARGIN);
            let px = |v: f32| (v * SCALE).round().max(0.0) as u32;
            let (x, y) = (px(rect.min.x), px(rect.min.y));
            let w = px(rect.width()).min(full.width().saturating_sub(x));
            let h = px(rect.height()).min(full.height().saturating_sub(y));
            image::imageops::crop_imm(&full, x, y, w, h).to_image()
        }
    }
}

/// Writes `image` as `<name>.png` unless the file already holds the same
/// pixels. Returns whether the file changed.
fn write_if_different(name: &str, image: &image::RgbaImage) -> bool {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder as _;
    let path = images_dir().join(format!("{name}.png"));
    if let Ok(existing) = image::open(&path) {
        if existing.to_rgba8() == *image {
            return false;
        }
    }
    std::fs::create_dir_all(images_dir()).expect("images folder");
    let file = std::fs::File::create(&path).expect("create png");
    PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        CompressionType::Best,
        FilterType::Adaptive,
    )
    .write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("write png");
    true
}

/// Renders every shot (or the ones whose name contains `STOKESFORGE_SHOT`)
/// into `manual/src/images/`. Run by `scripts/manual-screenshots.sh`.
#[test]
#[ignore = "needs a GPU adapter (or lavapipe); run by scripts/manual-screenshots.sh"]
fn render_manual_screenshots() {
    let _lock = capture_lock();
    // Nothing of the user's profile is read or written.
    let profile = tempfile::tempdir().expect("temporary profile");
    std::env::set_var(
        stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
        profile.path(),
    );
    std::env::set_var("XDG_CONFIG_HOME", profile.path());
    std::env::set_var(i18n::LANGUAGE_VARIABLE, "en");
    let _language = i18n::test_language(Language::English);
    let only = std::env::var("STOKESFORGE_SHOT").ok();
    for shot in SHOTS {
        if only.as_deref().is_some_and(|f| !shot.name.contains(f)) {
            continue;
        }
        let image = render_shot(shot);
        let changed = write_if_different(shot.name, &image);
        eprintln!(
            "{} {}x{} {}",
            shot.name,
            image.width(),
            image.height(),
            if changed { "written" } else { "unchanged" }
        );
    }
}

/// The manifest `manual/screenshots.json` and [`SHOTS`] describe the same
/// images, in the same order: every shot has an entry, every entry names the
/// page it illustrates and its image exists. An entry with `"feature":
/// "python"` is rendered only by a build with that feature: it is a shot only
/// then (its image must exist anyway).
#[test]
fn the_manifest_and_the_shots_agree() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manual");
    let text = std::fs::read_to_string(root.join("screenshots.json")).expect("manifest");
    let entries: Vec<serde_json::Value> = serde_json::from_str(&text).expect("valid JSON");
    let field = |entry: &serde_json::Value, key: &str| {
        entry[key]
            .as_str()
            .unwrap_or_else(|| panic!("{entry}: `{key}` is missing"))
            .to_string()
    };
    let mut listed = Vec::new();
    for entry in &entries {
        let image = field(entry, "image");
        assert!(!field(entry, "shows").is_empty(), "{image}: empty `shows`");
        let page = field(entry, "page");
        assert!(
            root.join("src").join(&page).exists(),
            "{image}: no page {page}"
        );
        assert!(
            root.join("src/images").join(&image).exists(),
            "{image} is produced by scripts/manual-screenshots.sh and is missing"
        );
        let needs_python = entry["feature"].as_str() == Some("python");
        if cfg!(feature = "python") || !needs_python {
            listed.push(image.trim_end_matches(".png").to_string());
        }
    }
    let shots: Vec<String> = SHOTS.iter().map(|shot| shot.name.to_string()).collect();
    assert_eq!(
        listed, shots,
        "the manifest and SHOTS differ (names and order)"
    );
}
