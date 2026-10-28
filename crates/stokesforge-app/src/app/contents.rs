//! Window contents shared between the floating window and the detached
//! window (pitfall 6 of #115): [`tab_contents`] dispatches on the kind of
//! tab; [`note_contents`] and [`matrix_contents`] depend only on
//! `(ui, state, actions)`, never on a field of `StokesForgeApp` — the
//! `show_viewport_deferred` closure is `'static + Send + Sync`.
//!
//! Moved out of `app.rs` as is (#121).

use std::collections::HashMap;

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::GraphView;

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::graph_window::graph_drop_target;
use super::tab::Tab;
use super::workbook::book_contents;

/// Paints the contents of tab `tab`, whatever its origin — the floating
/// windows loop and the maximize area
/// ([`StokesForgeApp::ui`](eframe::App::ui)) both call it: **one** function
/// that knows how to paint a tab, never two implementations to keep in
/// sync. Takes no `&egui::Context` any more since the script console (the
/// only kind that ever needed one, for its `request_repaint`) became a
/// fixed panel — see [`StokesForgeApp::script_panel`](crate::app::StokesForgeApp::script_panel).
pub(super) fn tab_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    graph_views: &mut HashMap<NodeId, GraphView>,
    tab: Tab,
) {
    match tab {
        Tab::Book(id) => book_contents(ui, state, actions, id),
        Tab::Graph(id) => graph_drop_target(ui, state, actions, graph_views, id),
        Tab::Matrix(id) => matrix_contents(ui, state.tree(), id),
        Tab::Note(id) => note_contents(ui, state, actions, id),
    }
}

/// Contents of a [`Tab::Matrix`] — **documented placeholder**. Matrix
/// editing is explicitly out of scope for this task
/// (`docs/application/managers.md`): the window opens and takes focus
/// exactly like the three other kinds, but shows only a message instead of
/// an editor. `stokesforge-core::NodeKind::Matrix` has no content today
/// (just window metadata) — so there is literally nothing else to show
/// until this kind has values.
///
/// One implementation for the floating window and the detached window: it
/// reads only the tree, which both have at hand (`state.tree()` on one
/// side, the viewport snapshot on the other).
pub(super) fn matrix_contents(ui: &mut egui::Ui, _tree: &Tree, _matrix: NodeId) {
    ui.centered_and_justified(|ui| {
        ui.weak(tr!("matrix-unavailable"));
    });
}

/// Contents of a [`Tab::Note`]: a multi-line editor, on the same deferred
/// input principle as `cell_widget` (`grid.rs`) — the buffer
/// ([`stokesforge_app_state::state::
/// NoteEdit`]) reaches the model only on focus loss, never on each
/// keystroke (otherwise the history would count one entry per character).
/// Escape abandons the input without writing anything, as for a cell.
pub(super) fn note_contents(
    ui: &mut egui::Ui,
    state: &mut AppState,
    actions: &mut Vec<Action>,
    note: NodeId,
) {
    let Some(current_name) = state.tree().node(note).map(|node| node.name().to_string()) else {
        ui.centered_and_justified(|ui| {
            ui.weak(tr!("note-deleted"));
        });
        return;
    };
    ui.heading(current_name);
    ui.separator();

    let editing_here = state.note_editing().is_some_and(|edit| edit.note == note);
    let mut buffer = if editing_here {
        state
            .note_editing()
            .map(|edit| edit.buffer.clone())
            .unwrap_or_default()
    } else {
        state.note_text(note).unwrap_or_default()
    };

    let response = ui.add(
        egui::TextEdit::multiline(&mut buffer)
            .desired_rows(20)
            .desired_width(f32::INFINITY),
    );

    if response.gained_focus() && !editing_here {
        state.begin_note_edit(note);
    }
    if state.note_editing().is_some_and(|edit| edit.note == note) {
        if let Some(edit) = state.note_editing_mut() {
            edit.buffer = buffer;
        }
    }

    if response.lost_focus() {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            state.cancel_note_edit();
        } else {
            let pending = state
                .note_editing()
                .filter(|edit| edit.note == note)
                .map(|edit| edit.buffer.clone());
            if let Some(text) = pending {
                state.cancel_note_edit();
                actions.push(Action::SetNoteText { note, text });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::painted;
    use super::*;
    use std::cell::RefCell;

    use stokesforge_core::Project;
    use stokesforge_session::Session;

    use super::super::project_manager::tests::find_text_center;

    /// A note "Log" with the text "Début" and a matrix.
    fn project() -> (Session, AppState, NodeId, NodeId) {
        let mut project = Project::new("Contents").unwrap();
        let root = project.root();
        let note = project.add_note(root, Some("Log"), "Début").unwrap();
        let matrix = project.add_matrix(root, Some("Map")).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (session, state, note, matrix)
    }

    /// A frame of `tab` on an 800 × 800 screen.
    fn paint(
        context: &egui::Context,
        state: &mut AppState,
        actions: &mut Vec<Action>,
        tab: Tab,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let mut graph_views = HashMap::new();
        let mut output = context.run_ui(input, |ui| {
            tab_contents(ui, state, actions, &mut graph_views, tab);
        });
        output.textures_delta.clear();
        output
    }

    /// `tab_contents` dispatches on the kind of tab: a note shows its name
    /// and its text, a matrix its placeholder message, and a vanished note
    /// says so instead of panicking.
    #[test]
    fn each_tab_paints_its_contents_and_a_vanished_note_says_so() {
        let (_session, mut state, note, matrix) = project();
        let context = egui::Context::default();
        let mut actions = Vec::new();
        let output = paint(
            &context,
            &mut state,
            &mut actions,
            Tab::Note(note),
            Vec::new(),
        );
        assert!(painted(&output, "Log") && painted(&output, "Début"));
        assert!(!painted(&output, &tr!("matrix-unavailable")));

        let output = paint(
            &context,
            &mut state,
            &mut actions,
            Tab::Matrix(matrix),
            Vec::new(),
        );
        assert!(painted(&output, &tr!("matrix-unavailable")));
        assert!(!painted(&output, "Log"));

        let gone = NodeId::from_raw(9999);
        let output = paint(
            &context,
            &mut state,
            &mut actions,
            Tab::Note(gone),
            Vec::new(),
        );
        assert!(painted(&output, &tr!("note-deleted")));
        assert!(actions.is_empty(), "painting writes nothing");
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Deferred input of a note: typing does not reach the model, losing
    /// focus commits in **one** action, Escape abandons without writing
    /// anything — like a cell.
    #[test]
    fn the_note_writes_the_model_only_on_focus_loss_and_escape_abandons() {
        let (_session, state, note, _) = project();
        let state = RefCell::new(state);
        let actions = RefCell::new(Vec::new());
        let context = egui::Context::default();
        let image = |events| {
            paint(
                &context,
                &mut state.borrow_mut(),
                &mut actions.borrow_mut(),
                Tab::Note(note),
                events,
            )
        };
        let first = image(Vec::new());
        // To the right of the text, on its line: the cursor lands at the end of the text.
        let at = find_text_center(&first.shapes, "Début") + egui::vec2(200.0, 0.0);
        let outside = egui::pos2(400.0, 790.0);
        let edit = |text: &str| {
            image(vec![egui::Event::PointerMoved(at), pointer(at, true)]);
            image(vec![pointer(at, false)]);
            image(vec![egui::Event::Text(text.to_owned())]);
        };

        edit(" suite");
        assert!(
            actions.borrow().is_empty(),
            "typing does not write the model"
        );
        assert_eq!(
            state
                .borrow()
                .note_editing()
                .map(|edit| edit.buffer.clone()),
            Some("Début suite".to_owned())
        );
        image(vec![
            egui::Event::PointerMoved(outside),
            pointer(outside, true),
        ]);
        image(vec![pointer(outside, false)]);
        assert_eq!(
            *actions.borrow(),
            vec![Action::SetNoteText {
                note,
                text: "Début suite".to_owned(),
            }],
            "focus loss commits in one action"
        );
        assert!(state.borrow().note_editing().is_none());

        actions.borrow_mut().clear();
        edit(" dropped");
        image(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(actions.borrow().is_empty(), "Escape writes nothing");
        assert!(
            state.borrow().note_editing().is_none(),
            "Escape abandons the input"
        );
    }
}
