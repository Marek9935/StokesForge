//! Every dialog opens in the main window (#472, work package #470).
//!
//! A gesture made in a detached window may open a dialog; the root paints it,
//! the detached window shows a band that says where it is. These tests drive a
//! root and one detached graph the way eframe does (`test_support::bench`).
use std::time::{Duration, Instant};

use eframe::egui;
use stokesforge_app_state::state::Action;
use stokesforge_app_state::tr;

use super::test_support::bench::{delay, Bench, Pass};
use super::test_support::{painted, text_exact, texts};
use super::window_band;

fn ok() -> String {
    tr!("dialog-ok")
}

fn band_text(bench: &Bench) -> Option<String> {
    window_band::text_at(&bench.ctx, bench.child_id(), Instant::now()).map(|(text, _)| text)
}

fn focus_commands(output: &egui::FullOutput) -> usize {
    output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .iter()
        .filter(|command| matches!(command, egui::ViewportCommand::Focus))
        .count()
}

fn click_at(bench: &mut Bench, last: &egui::FullOutput, label: &str) -> egui::FullOutput {
    let pos = text_exact(&texts(&last.shapes), label).center;
    let mut output = None;
    for pressed in [None, Some(true), Some(false)] {
        let events = match pressed {
            None => vec![egui::Event::PointerMoved(pos)],
            Some(pressed) => vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        };
        output = Some(bench.root(Pass::root().events(events)));
    }
    output.unwrap()
}

/// Case 1. A detached window queues a dialog that changes no snapshot (About):
/// the root is woken, paints it on a later pass, the window never does; at
/// rest everything sleeps. Witness: remove the two commands sent to the root by
/// `announce_opened_dialog`.
#[test]
fn a_dialog_asked_by_a_detached_window_is_painted_by_the_root_only() {
    let mut bench = Bench::new();
    bench.start();
    let mut rest = bench.root(Pass::root());
    for _ in 0..12 {
        rest = bench.root(Pass::root());
    }
    assert_eq!(delay(&rest, bench.child_id()), Duration::MAX, "asleep");
    assert_eq!(
        delay(&rest, egui::ViewportId::ROOT),
        Duration::MAX,
        "root asleep"
    );
    bench.queue_in_window(Action::OpenAbout);
    let opening = bench.root(Pass::root());
    assert!(bench.app.state.about_open());
    assert_eq!(
        delay(&opening, egui::ViewportId::ROOT),
        Duration::ZERO,
        "the root is woken to paint it"
    );
    let mut root = opening;
    for _ in 0..5 {
        root = bench.root(Pass::root());
    }
    assert!(painted(&root, &tr!("dialog-close")), "the root paints it");
    let child = bench.child(Pass::child());
    assert!(
        !painted(&child, &tr!("dialog-close")),
        "the window never does"
    );
    bench.finish();
}

/// Case 2. OK clicked in the root: the detached window already holds the new
/// snapshot in that very pass. Witness: remove the republication after a
/// change of the stamp.
#[test]
fn ok_in_the_root_reaches_the_detached_window_in_the_same_pass() {
    use super::window_detach::snapshot_stamp;
    let mut bench = Bench::new();
    bench.start();
    bench.queue_in_window(bench.open_plot_settings());
    let mut root = bench.root(Pass::root());
    for _ in 0..5 {
        root = bench.root(Pass::root());
    }
    bench.app.state.plot_edit_mut().unwrap().spec.title = "Edited".into();
    let before = snapshot_stamp(bench.app.state.snapshot());
    let last = click_at(&mut bench, &root, &ok());
    assert!(bench.app.state.plot_settings().is_none());
    let after = snapshot_stamp(bench.app.state.snapshot());
    assert_ne!(before, after);
    let published = bench.app.detached_tabs[&bench.tab]
        .view_model
        .lock()
        .unwrap()
        .snapshot
        .as_ref()
        .map(snapshot_stamp);
    assert_eq!(published, Some(after));
    assert_eq!(
        delay(&last, bench.child_id()),
        Duration::ZERO,
        "the window is woken"
    );
    bench.finish();
}

/// Case 3. A dialog opened while the root gets no focus is painted in full
/// once the root is focused again, and its first field takes the typed text.
/// Witness: end the focus walk (`advance_initial_focus`) after the first pass.
#[test]
fn a_dialog_opened_while_the_root_was_unfocused_paints_in_full_and_takes_typing() {
    let mut bench = Bench::new();
    bench.start();
    bench.queue_in_window(bench.open_plot_settings());
    for _ in 0..4 {
        bench.root(Pass::root().dt(0.11));
    }
    assert!(bench.app.state.plot_settings().is_some());
    let mut root = bench.root(Pass::root().focused(true));
    for _ in 0..5 {
        root = bench.root(Pass::root().focused(true));
    }
    assert!(painted(&root, &ok()), "painted in full");
    bench.root(
        Pass::root()
            .focused(true)
            .events(vec![egui::Event::Text("Zed".into())]),
    );
    bench.root(Pass::root().focused(true));
    let title = bench.app.state.plot_edit().unwrap().spec.title.clone();
    assert!(
        title.contains("Zed"),
        "the first field has the focus: {title:?}"
    );
    bench.finish();
}

/// Case 4. Opening from the detached window sends exactly one `Focus` to the
/// root; later passes and a dialog opened from the root send none. Witness:
/// remove the command, then send it on every pass.
#[test]
fn the_main_window_is_asked_to_come_forward_once() {
    let mut bench = Bench::new();
    bench.start();
    bench.queue_in_window(bench.open_plot_settings());
    let opening = bench.root(Pass::root());
    assert_eq!(focus_commands(&opening), 1);
    for _ in 0..3 {
        assert_eq!(focus_commands(&bench.root(Pass::root())), 0);
    }
    bench.app.state.cancel_plot_settings();
    bench.root(Pass::root());
    bench.child(Pass::child().focused(false));
    bench.app.state.dispatch(Action::OpenAbout);
    let from_root = bench.root(Pass::root().focused(true));
    assert_eq!(focus_commands(&from_root), 0, "opened from the root");
    bench.finish();
}

/// Case 5. The band names the dialog, in the window of the gesture only, and
/// is gone after five seconds. Witness: post it in the root as well.
#[test]
fn the_window_of_the_gesture_says_where_the_dialog_is() {
    let mut bench = Bench::new();
    bench.start();
    bench.queue_in_window(bench.open_plot_settings());
    bench.root(Pass::root());
    let text = band_text(&bench).expect("a band in the window");
    assert!(
        text.contains(&tr!("dialog-name-graph-properties")),
        "{text}"
    );
    let now = Instant::now();
    assert!(window_band::text_at(&bench.ctx, egui::ViewportId::ROOT, now).is_none());
    assert!(
        window_band::text_at(&bench.ctx, bench.child_id(), now + Duration::from_secs(6)).is_none(),
        "gone after five seconds"
    );
    bench.finish();
}

/// Case 5b (D2). A question that opens on its own while the person works in a
/// detached window is announced in that window.
#[test]
fn a_question_that_opens_alone_is_announced_in_the_focused_window() {
    let mut bench = Bench::new();
    bench.start();
    bench.app.state.dispatch(Action::NewProject {
        pending_edits: true,
    });
    bench.child(Pass::child());
    bench.root(Pass::root());
    let text = band_text(&bench).expect("a band");
    assert_eq!(text, tr!("opened-question-in-main"));
    bench.finish();
}

/// Case 6. Ctrl+Z in a text field of a dialog undoes the text and sends no
/// `Undo`. Witness: remove the dialog gate of `collect_shortcuts`.
#[test]
fn ctrl_z_in_a_dialog_field_undoes_the_text_and_not_the_model() {
    let mut bench = Bench::new();
    bench.start();
    bench.queue_in_window(bench.open_plot_settings());
    let mut root = bench.root(Pass::root());
    for _ in 0..5 {
        root = bench.root(Pass::root().focused(true));
    }
    let field = text_exact(&texts(&root.shapes), "Figure").center;
    for pressed in [None, Some(true), Some(false)] {
        let events = match pressed {
            None => vec![egui::Event::PointerMoved(field)],
            Some(pressed) => vec![egui::Event::PointerButton {
                pos: field,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        };
        bench.root(Pass::root().focused(true).events(events));
    }
    bench.root(
        Pass::root()
            .focused(true)
            .dt(2.0)
            .events(vec![egui::Event::Text("X".into())]),
    );
    for _ in 0..2 {
        bench.root(Pass::root().focused(true).dt(2.0));
    }
    let typed = bench.app.state.plot_edit().unwrap().spec.title.clone();
    assert!(typed.contains('X'), "typed: {typed:?}");
    let revision = bench.app.state.revision();
    let undo = egui::Event::Key {
        key: egui::Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    };
    bench.root(Pass::root().focused(true).dt(2.0).events(vec![undo]));
    bench.root(Pass::root().focused(true).dt(2.0));
    let after = bench.app.state.plot_edit().unwrap().spec.title.clone();
    assert!(!after.contains('X'), "the field undid its text: {after:?}");
    assert_eq!(bench.app.state.revision(), revision, "no model undo");
    bench.finish();
}

/// Case 7. A confirmation is open: the action of the detached window is
/// dropped and the root paints the question. Witness: remove the clearing in
/// `blocking_confirmations`.
#[test]
fn a_confirmation_drops_the_detached_action_and_is_painted_by_the_root() {
    let mut bench = Bench::new();
    bench.start();
    bench.root(Pass::root());
    bench.app.state.dispatch(Action::NewProject {
        pending_edits: true,
    });
    let books = bench.app.state.books().len();
    bench.queue_in_window(Action::AddWorkbook);
    let mut root = bench.root(Pass::root());
    for _ in 0..3 {
        root = bench.root(Pass::root());
    }
    assert_eq!(bench.app.state.books().len(), books, "dropped");
    assert!(painted(&root, &tr!("dialog-cancel")), "painted by the root");
    bench.finish();
}

/// Case 8. Reattaching or closing the window that opened a dialog leaves the
/// dialog where it is, and its button still answers. Witness: close the dialog
/// in `close_detached_tab`.
#[test]
fn closing_the_window_that_opened_a_dialog_leaves_the_dialog() {
    let mut bench = Bench::new();
    bench.start();
    bench.queue_in_window(bench.open_plot_settings());
    let mut root = bench.root(Pass::root());
    for _ in 0..5 {
        root = bench.root(Pass::root());
    }
    let tab = bench.tab;
    bench.app.close_detached_tab(tab);
    for _ in 0..3 {
        root = bench.root(Pass::root());
    }
    assert!(bench.app.state.plot_settings().is_some(), "still open");
    assert!(painted(&root, &ok()));
    click_at(&mut bench, &root, &ok());
    assert!(bench.app.state.plot_settings().is_none(), "answered");
    bench.finish();
}

/// A dialog squeezed by a small main window is not remembered at that size.
/// Witness: remove the `clamped` test from `modal_frame`.
#[test]
fn a_dialog_closed_in_a_small_window_keeps_the_size_the_user_chose() {
    use super::dialogs::remembered_size;
    let plot = egui::Id::new("stokesforge_plot_settings");
    let mut bench = Bench::new();
    bench.start();
    let open_and_cancel = |bench: &mut Bench, size: (f32, f32)| {
        bench.app.state.dispatch(bench.open_plot_settings());
        let mut root = bench.root(Pass::root().size(size.0, size.1));
        for _ in 0..5 {
            root = bench.root(Pass::root().size(size.0, size.1));
        }
        let at = text_exact(&texts(&root.shapes), &tr!("dialog-cancel")).center;
        for pressed in [None, Some(true), Some(false)] {
            let events = match pressed {
                None => vec![egui::Event::PointerMoved(at)],
                Some(pressed) => vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            };
            bench.root(Pass::root().size(size.0, size.1).events(events));
        }
        bench.root(Pass::root().size(size.0, size.1));
        assert!(bench.app.state.plot_settings().is_none(), "closed");
    };
    open_and_cancel(&mut bench, (1400.0, 900.0));
    let remembered = remembered_size(&bench.ctx, plot).expect("remembered");
    open_and_cancel(&mut bench, (560.0, 300.0));
    assert_eq!(remembered_size(&bench.ctx, plot), Some(remembered));
    bench.finish();
}
