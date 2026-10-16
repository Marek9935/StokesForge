//! Contenus de fenêtre partagés entre fenêtre flottante et fenêtre détachée
//! (piège 6 de #115) : [`tab_contents`] aiguille selon la nature de
//! l'onglet ; [`note_contents`] et [`matrix_contents`] ne dépendent que de
//! `(ui, état, actions)`, jamais d'un champ de `StokesForgeApp` — la fermeture
//! de `show_viewport_deferred` est `'static + Send + Sync`.
//!
//! Sorti d'`app.rs` tel quel (#121).

use std::collections::HashMap;

use eframe::egui;
use stokesforge_core::{NodeId, Tree};
use stokesforge_render_egui::GraphView;

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::graph_window::graph_drop_target;
use super::tab::Tab;
use super::workbook::book_contents;

/// Peint le contenu de l'onglet `tab`, quelle que soit sa provenance — la
/// boucle des fenêtres flottantes ou la zone de maximisation
/// ([`StokesForgeApp::ui`](eframe::App::ui)) l'appellent toutes les deux : **une seule** fonction
/// qui sait peindre un onglet, jamais deux implémentations à tenir en
/// accord. Ne prend plus de `&egui::Context` depuis que la console de script
/// (seule nature à en avoir eu besoin, pour son `request_repaint`) est
/// devenue un panneau fixe — voir [`StokesForgeApp::script_panel`](crate::app::StokesForgeApp::script_panel).
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

/// Contenu d'un [`Tab::Matrix`] — **espace réservé documenté**. L'édition de
/// matrice est explicitement hors périmètre de cette tâche
/// (`docs/application.md`) : la fenêtre s'ouvre et se focalise exactement
/// comme les trois autres natures, mais ne montre qu'un message plutôt qu'un
/// éditeur. `stokesforge-core::NodeKind::Matrix` n'a aujourd'hui aucun contenu
/// (juste des métadonnées de fenêtre) — il n'y a donc littéralement rien
/// d'autre à afficher tant que cette nature n'a pas de valeurs.
///
/// Une seule écriture pour la fenêtre flottante et la fenêtre détachée : elle
/// ne lit que l'arbre, que les deux ont sous la main (`state.tree()` d'un
/// côté, l'instantané du viewport de l'autre).
pub(super) fn matrix_contents(ui: &mut egui::Ui, _tree: &Tree, _matrix: NodeId) {
    ui.centered_and_justified(|ui| {
        ui.weak(tr!("matrix-unavailable"));
    });
}

/// Contenu d'un [`Tab::Note`] : un éditeur multi-lignes, sur le même principe
/// de saisie différée que `cell_widget` (`grid.rs`) — le tampon ([`stokesforge_app_state::state::
/// NoteEdit`]) n'atteint le modèle qu'à la perte de focus, jamais à chaque
/// frappe (sinon l'historique compterait une entrée par caractère). Échap
/// abandonne la saisie sans rien écrire, comme pour une cellule.
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

    /// Une note « Journal » au texte « Début » et une matrice.
    fn projet() -> (Session, AppState, NodeId, NodeId) {
        let mut project = Project::new("Contenus").unwrap();
        let root = project.root();
        let note = project.add_note(root, Some("Journal"), "Début").unwrap();
        let matrix = project.add_matrix(root, Some("Carte")).unwrap();
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (session, state, note, matrix)
    }

    /// Une image de `tab` sur un écran de 800 × 800.
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

    /// `tab_contents` aiguille selon la nature de l'onglet : une note montre
    /// son nom et son texte, une matrice son message d'attente, et une note
    /// disparue le dit au lieu de paniquer.
    #[test]
    fn chaque_onglet_peint_son_contenu_et_une_note_disparue_le_dit() {
        let (_session, mut state, note, matrix) = projet();
        let context = egui::Context::default();
        let mut actions = Vec::new();
        let output = paint(
            &context,
            &mut state,
            &mut actions,
            Tab::Note(note),
            Vec::new(),
        );
        assert!(painted(&output, "Journal") && painted(&output, "Début"));
        assert!(!painted(&output, &tr!("matrix-unavailable")));

        let output = paint(
            &context,
            &mut state,
            &mut actions,
            Tab::Matrix(matrix),
            Vec::new(),
        );
        assert!(painted(&output, &tr!("matrix-unavailable")));
        assert!(!painted(&output, "Journal"));

        let gone = NodeId::from_raw(9999);
        let output = paint(
            &context,
            &mut state,
            &mut actions,
            Tab::Note(gone),
            Vec::new(),
        );
        assert!(painted(&output, &tr!("note-deleted")));
        assert!(actions.is_empty(), "peindre n'écrit rien");
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Saisie différée d'une note : la frappe n'atteint pas le modèle, la
    /// perte de focus valide en **une** action, Échap abandonne sans rien
    /// écrire — comme une cellule.
    #[test]
    fn la_note_n_ecrit_le_modele_qu_a_la_perte_de_focus_et_echap_abandonne() {
        let (_session, state, note, _) = projet();
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
        // À droite du texte, sur sa ligne : le curseur se pose en fin de texte.
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
            "la frappe n'écrit pas le modèle"
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
            "la perte de focus valide en une action"
        );
        assert!(state.borrow().note_editing().is_none());

        actions.borrow_mut().clear();
        edit(" jetée");
        image(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(actions.borrow().is_empty(), "Échap n'écrit rien");
        assert!(
            state.borrow().note_editing().is_none(),
            "Échap abandonne la saisie"
        );
    }
}
