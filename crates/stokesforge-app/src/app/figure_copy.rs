//! Copy the figure of the active graph to an external application (Ctrl+C,
//! issue #149).
//!
//! # The gesture
//!
//! Ctrl+C never arrives as an `Event::Key`: `egui-winit` translates it into
//! `Event::Copy` (see `grid.rs`, `grid_clipboard_shortcuts`). The whole
//! figure is copied when, **together**:
//!
//! - a graph window is active, in the foreground — floating (the one that
//!   `active_content_tab` designates) or detached (its viewport receives the
//!   key, so it is the focused one);
//! - no graph object is selected (`selected_manager_target`: series, free
//!   text, shape, title or legend; the Object Manager selection is the
//!   view's): copy-paste of an object is not defined by #149;
//! - no input field has focus (`keyboard_focus_outside_grid`): a Ctrl+C in
//!   a field copies its text.
//!
//! The Project Manager removes its own `Event::Copy` when it has focus, and
//! this module runs **before** the panels and the contents (the shortcut is
//! consumed at the context level, like `collect_shortcuts`): a grid visible
//! behind the graph window therefore does not receive the same
//! `Event::Copy` and does not overwrite the figure with cell text.
//!
//! # What this module does not do
//!
//! It neither renders nor offers anything: it pushes an
//! [`Action::CopyGraphFigure`] whose spec is already framed by the view (the
//! displayed zoom, like the export to a file). `AppState` prepares the bytes,
//! and [`StokesForgeApp::offer_figure`] offers them to the system from the
//! root window, the only one that knows its display (`eframe::Frame`). No
//! command, no history entry.

use eframe::egui;
use stokesforge_clipboard::{mime, ClipboardError, Representation};
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::GraphView;
use stokesforge_render_svg::ExportFormat;

use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;

use super::grid::keyboard_focus_outside_grid;
use super::object_manager_ui::selected_manager_target;
use super::tab::Tab;
use super::StokesForgeApp;

/// True if an `Event::Copy` is waiting in the context input — the cheap read
/// that avoids any work on frames without Ctrl+C.
pub(super) fn copy_event_pending(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Copy))
    })
}

/// Is this the "copy the figure" gesture? If so, it **removes** the
/// `Event::Copy` so that nobody else handles it.
///
/// `view` is the view of the graph that the gesture would target: its selection decides.
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

/// Pushes [`Action::CopyGraphFigure`] if the gesture is the one for the figure of
/// `graph`, framed by `view`.
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
    /// Ctrl+C on the active floating graph (the root receives the key only
    /// if it has focus; a detached graph reads its own in its viewport, see
    /// `window_detach.rs`).
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

    /// The "Copy graph" entry of the menus: the same action as Ctrl+C,
    /// with no selection condition (the gesture is explicit), with the framing of
    /// the view of graph `graph`, floating or detached.
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

    /// Offers to the system the representations that [`Action::CopyGraphFigure`]
    /// has just prepared. A refusal replaces the "copied" message with its cause.
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

/// The MIME type under which the figure of an export format is offered. An
/// exhaustive `match`, without `_`: a new format does not compile until it
/// has its type.
fn mime_of(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Svg { .. } => mime::SVG,
        ExportFormat::Png { .. } => mime::PNG,
        ExportFormat::Pdf => mime::PDF,
    }
}

/// The sentence of a [`ClipboardError`]: the errors of the multi-format offer
/// to the system clipboard (#149), put into sentences here because
/// `stokesforge-clipboard` (and its winit) stays in the application.
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
    use super::super::graph_window::{graph_view_contents, tests::plotted_graph};
    use super::super::test_support::{click, context, frame_of, image};
    use super::*;
    use stokesforge_app_state::errors::tests_support::check;
    use stokesforge_app_state::state::AppState;
    use stokesforge_session::Session;

    /// Each offered export format has its MIME type, distinct from the others
    /// (witness: two formats sharing a type would be indistinguishable
    /// for the receiving application).
    #[test]
    fn each_figure_format_has_its_mime_type() {
        use stokesforge_render_svg::SvgTextMode;
        let svg = ExportFormat::Svg {
            text: SvgTextMode::Text,
        };
        let png = ExportFormat::Png { scale: 2.0 };
        assert_eq!(mime_of(svg), "image/svg+xml");
        assert_eq!(mime_of(png), "image/png");
        assert_eq!(mime_of(ExportFormat::Pdf), "application/pdf");
        // The PNG scale does not change its type.
        assert_eq!(mime_of(png), mime_of(ExportFormat::Png { scale: 1.0 }));
    }

    #[test]
    fn each_clipboard_error_has_its_sentence() {
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
        assert!(seen.iter().all(|&s| s), "a variant has no sample");
        // The detail and the display are carried over as is into the sentence.
        let phrase = clipboard_error_message(&ClipboardError::Backend("selection refused".into()));
        assert!(phrase.contains("selection refused"), "{phrase}");
        let phrase = clipboard_error_message(&ClipboardError::Unsupported("Wayland"));
        assert!(phrase.contains("Wayland"), "{phrase}");
    }

    /// An application whose graph is in the foreground, and its view already
    /// created (a view is born at the first paint of the window).
    fn bench() -> (Session, StokesForgeApp, NodeId) {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let mut app = StokesForgeApp::new(state);
        app.graph_views.insert(graph, GraphView::new());
        assert_eq!(app.floating_tabs.last(), Some(&Tab::Graph(graph)));
        (session, app, graph)
    }

    /// A frame of 900 × 700 with `events`, textures discarded.
    fn input(ctx: &egui::Context, events: Vec<egui::Event>, content: impl FnMut(&mut egui::Ui)) {
        frame_of(ctx, egui::vec2(900.0, 700.0), events, content);
    }

    /// A frame with `events` where the application reads Ctrl+C for `tab`; returns
    /// the pushed actions and the events that remain for others.
    fn paint_app(
        app: &mut StokesForgeApp,
        ctx: &egui::Context,
        tab: Option<Tab>,
        events: Vec<egui::Event>,
    ) -> (Vec<Action>, Vec<egui::Event>) {
        let mut actions = Vec::new();
        let mut remaining = Vec::new();
        input(ctx, events, |ui| {
            app.request_active_figure_copy(ui.ctx(), tab, &mut actions);
            remaining = ui.input(|input| input.events.clone());
        });
        (actions, remaining)
    }

    fn copy_events() -> Vec<egui::Event> {
        vec![egui::Event::Copy]
    }

    fn is_copy(events: &[egui::Event]) -> bool {
        events.iter().any(|e| matches!(e, egui::Event::Copy))
    }

    /// The intended gesture: active graph, nothing selected, Ctrl+C. A
    /// single action, with the figure framed by the view; the event is removed
    /// so that the grid of a workbook visible behind does not read it.
    #[test]
    fn ctrl_c_on_an_active_graph_with_no_selection_copies_the_figure() {
        let (session, mut app, graph) = bench();
        let ctx = context();
        let tab = Some(Tab::Graph(graph));

        // Witness: without Ctrl+C, nothing goes out.
        let (actions, _) = paint_app(&mut app, &ctx, tab, Vec::new());
        assert!(actions.is_empty(), "{actions:?}");

        let (actions, remaining) = paint_app(&mut app, &ctx, tab, copy_events());
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        let framed = app
            .read_graph_view(graph, |v| v.framed_spec(&spec))
            .unwrap();
        assert_eq!(
            actions,
            vec![Action::CopyGraphFigure {
                spec: Box::new(framed)
            }]
        );
        assert!(!is_copy(&remaining), "the event must be consumed");
        session.shutdown().unwrap();
    }

    /// A selected object keeps Ctrl+C: copy-paste of graphical objects is not
    /// defined by #149, and the event stays readable.
    #[test]
    fn a_selected_object_keeps_ctrl_c() {
        let (session, mut app, graph) = bench();
        let ctx = context();
        let tab = Some(Tab::Graph(graph));
        let spec = stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap();
        let series = stokesforge_plot::SeriesRef::new(0, 0);
        assert!(
            spec.series_at(series).is_some(),
            "witness: the series exists"
        );
        app.with_graph_view(graph, |view| view.select_series(Some(series), &spec));
        let (actions, remaining) = paint_app(&mut app, &ctx, tab, copy_events());
        assert!(actions.is_empty(), "{actions:?}");
        assert!(is_copy(&remaining), "the event must stay");

        // Once deselected, the figure goes out: the selection was indeed
        // what decided.
        app.with_graph_view(graph, |view| view.select_series(None, &spec));
        let (actions, _) = paint_app(&mut app, &ctx, tab, copy_events());
        assert_eq!(actions.len(), 1);
        session.shutdown().unwrap();
    }

    /// A focused input field keeps Ctrl+C: it copies its text.
    #[test]
    fn an_input_field_keeps_ctrl_c() {
        let (session, mut app, graph) = bench();
        let ctx = context();
        let tab = Some(Tab::Graph(graph));
        let id = egui::Id::new("field");
        let mut text = String::from("abc");
        let mut paint_field = |app: &mut StokesForgeApp, events: Vec<egui::Event>, focus: bool| {
            let mut actions = Vec::new();
            let mut remaining = Vec::new();
            input(&ctx, events, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(id));
                if focus {
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                app.request_active_figure_copy(ui.ctx(), tab, &mut actions);
                remaining = ui.input(|input| input.events.clone());
            });
            (actions, remaining)
        };
        paint_field(&mut app, Vec::new(), true);
        let (actions, remaining) = paint_field(&mut app, copy_events(), false);
        assert!(actions.is_empty(), "{actions:?}");
        assert!(is_copy(&remaining), "the event must stay with the field");
        session.shutdown().unwrap();
    }

    /// A workbook in the foreground, or no window: no figure.
    #[test]
    fn only_an_active_graph_copies_its_figure() {
        let (session, mut app, _graph) = bench();
        let ctx = context();
        let book = app.state.books()[0].0;
        let (actions, remaining) = paint_app(&mut app, &ctx, Some(Tab::Book(book)), copy_events());
        assert!(actions.is_empty(), "{actions:?}");
        assert!(is_copy(&remaining));
        let (actions, _) = paint_app(&mut app, &ctx, None, copy_events());
        assert!(actions.is_empty(), "{actions:?}");
        session.shutdown().unwrap();
    }

    /// The targeted figure is the one of the active tab, not of another open
    /// graph: Ctrl+C on the second gives the spec of the second.
    #[test]
    fn the_copied_figure_is_the_one_of_the_active_graph() {
        let (session, mut app, first) = bench();
        app.state.dispatch(Action::AddGraph);
        let second = app.state.graphs().last().unwrap().0;
        assert_ne!(first, second);
        app.graph_views.insert(second, GraphView::new());
        let ctx = context();
        let spec_of = |app: &StokesForgeApp, graph| {
            stokesforge_core::compose::current_plot_spec(app.state.tree(), graph).unwrap()
        };
        let (actions, _) = paint_app(&mut app, &ctx, Some(Tab::Graph(second)), copy_events());
        assert_eq!(
            actions,
            vec![Action::CopyGraphFigure {
                spec: Box::new(spec_of(&app, second))
            }]
        );
        assert_ne!(spec_of(&app, first), spec_of(&app, second), "witness");
        session.shutdown().unwrap();
    }

    /// Clicking on the figure leaves no keyboard focus to a widget of the
    /// graph: otherwise Ctrl+C, right after a click on the figure, would be
    /// taken for input and would never copy.
    #[test]
    fn clicking_on_the_figure_does_not_take_keyboard_focus() {
        let (session, graph) = plotted_graph();
        let state = AppState::new(session.handle(), "test.stf".into()).unwrap();
        let ctx = context();
        let mut view = GraphView::new();
        let mut actions = Vec::new();
        let mut paint_graph = |ui: &mut egui::Ui| {
            graph_view_contents(ui, &mut view, state.snapshot(), graph, &mut actions);
        };
        image(&ctx, Vec::new(), &mut paint_graph);
        let center = egui::pos2(400.0, 300.0);
        click(&ctx, center, egui::PointerButton::Primary, &mut paint_graph);
        assert_eq!(ctx.memory(|memory| memory.focused()), None);
        assert!(!keyboard_focus_outside_grid(&ctx));

        // Witness: the test can see a focus when there is one.
        let elsewhere = egui::Id::new("elsewhere");
        let mut field = |ui: &mut egui::Ui| {
            ui.add(egui::TextEdit::singleline(&mut String::new()).id(elsewhere));
            ui.memory_mut(|memory| memory.request_focus(elsewhere));
        };
        image(&ctx, Vec::new(), &mut field);
        image(&ctx, Vec::new(), &mut field);
        assert!(keyboard_focus_outside_grid(&ctx));
        session.shutdown().unwrap();
    }
}
